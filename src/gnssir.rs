use std::collections::{HashMap, VecDeque};
use std::iter::Map;
use scirs2_signal::lombscargle::{lombscargle, AutoFreqMethod};
use ndarray::Array1;
use std::f64::consts::PI;

use crate::db::record::Record;
use crate::db::arc::{self, Arc};

pub fn find_arcs(records: VecDeque<Record>) -> Vec<Arc> {
    let n_records = records.len();
    if n_records == 0 {
        return Vec::new();
    }

    // 1) Group indices by satellite ID.
    let mut by_id: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, rec) in records.iter().enumerate() {
        by_id.entry(rec.id).or_default().push(i);
    }

    let mut arcs = Vec::new();

    for (id, mut idxs) in by_id {
        // 2) Ensure temporal order per ID (important if input isn't already time-sorted).
        idxs.sort_by_key(|&i| records[i].time);

        // 3) Walk through, splitting on >120s gaps.
        let mut current_arc_indices: Vec<usize> = Vec::new();
        let mut arc_start_time: i64 = 0;
        let mut last_time: i64 = 0;

        for &i in &idxs {
            let t = records[i].time;

            if current_arc_indices.is_empty() {
                // start a new arc
                current_arc_indices.push(i);
                arc_start_time = t;
                last_time = t;
                continue;
            }

            if t - last_time > 120 {
                // finalize previous arc
                let arc_indices = std::mem::take(&mut current_arc_indices);
                println!("Finalizing arc for ID {}: {} records from {} to {}", id, arc_indices.len(), arc_start_time, last_time);
                arcs.push(Arc::new(id, arc_start_time, last_time, arc_indices));

                // start next arc
                current_arc_indices.push(i);
                arc_start_time = t;
            } else {
                current_arc_indices.push(i);
            }

            last_time = t;
        }

        // 4) Emit the final arc for this ID (if any).
        if !current_arc_indices.is_empty() {
            let arc_indices = std::mem::take(&mut current_arc_indices);
            println!("Finalizing arc for ID {}: {} records from {} to {}", id, arc_indices.len(), arc_start_time, last_time);
            arcs.push(Arc::new(id, arc_start_time, last_time, arc_indices));
        }
    }

    arcs
}

use polyfit_rs::polyfit_rs::polyfit;

pub fn fix_arc_elev_azim(arc: &Arc, records: &mut VecDeque<Record>) {
    // Fit a 3rd order polynomial to elevation and azimuth over the arc, then interpolate

    // Collect times, elevations, and azimuths for the arc
    let mut times = Vec::with_capacity(arc.record_indices.len());
    let mut elevs = Vec::with_capacity(arc.record_indices.len());
    let mut azims = Vec::with_capacity(arc.record_indices.len());

    for &idx in &arc.record_indices {
        if let Some(rec) = records.get(idx) {
            times.push(rec.time as f64);
            elevs.push(rec.elevation as f64);
            azims.push(rec.azimuth as f64);
        }
    }

    // Fit 3rd order polynomials
    let elev_poly = polyfit(&times, &elevs, 3).unwrap_or(vec![0.0; 4]);
    let azim_poly = polyfit(&times, &azims, 3).unwrap_or(vec![0.0; 4]);

    // Helper to evaluate polynomial
    fn eval_poly(coeffs: &[f64], x: f64) -> f64 {
        coeffs.iter().rev().fold(0.0, |acc, &c| acc * x + c)
    }

    // Write interpolated values back
    for (&idx, &t) in arc.record_indices.iter().zip(times.iter()) {
        if let Some(rec) = records.get_mut(idx) {
            let new_elev = eval_poly(&elev_poly, t);
            let new_azim = eval_poly(&azim_poly, t);
            //println!("Arc ID {}: Updating record at time {}: elev {:.2} -> {:.2}, azim {:.2} -> {:.2}", arc.sat_id, t as i64, rec.elevation, new_elev, rec.azimuth, new_azim);
            rec.elevation = new_elev;
            rec.azimuth = new_azim;
        }
    }
}

pub fn correct_arc_snr(arc: &Arc, records: &mut VecDeque<Record>) {
    // Collect SNR values for the arc's records
    let snr_values: Vec<f64> = arc
        .record_indices
        .iter()
        .filter_map(|&idx| records.get(idx).map(|rec| rec.snr))
        .collect();

    // Detrend SNR using a 3rd order polynomial
    let detrended = match scirs2_signal::detrend::detrend_poly(&snr_values, 3) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Detrending error for arc with sat_id {}: {}", arc.sat_id, e);
            return;
        }
    };

    // Write detrended SNR back to the records
    for (i, &idx) in arc.record_indices.iter().enumerate() {
        if let Some(rec) = records.get_mut(idx) {
            rec.snr = detrended[i];
        }
    }
}

use std::cmp::Ordering;

pub fn find_arc_frequencies(arc: &Arc, records: &VecDeque<Record>) -> Vec<(f64, f64)> {
    const MAX_NOUT: usize = 200_000; // cap to avoid runaway allocations
    let n = arc.record_indices.len();
    if n < 3 {
        eprintln!("Arc {}: too few points (n={}), skipping.", arc.sat_id, n);
        return Vec::new();
    }

    // Constants
    let l1_wv_m = 299_792_458.0 / (1575.42e6); // L1 wavelength (m)
    let cf = l1_wv_m / 2.0;

    // Collect (x,y) without mutating records
    let mut min_elev = 90.0f64;
    let mut max_elev = 0.0f64;
    let mut pairs: Vec<(f64, f64)> = Vec::with_capacity(n);

    for &idx in &arc.record_indices {
        if let Some(rec) = records.get(idx) {
            let xi = (rec.elevation.to_radians()).sin() / cf;
            let yi = rec.snr;

            if !xi.is_finite() || !yi.is_finite() {
                continue; // drop non-finite points
            }

            if rec.elevation < min_elev { min_elev = rec.elevation; }
            if rec.elevation > max_elev { max_elev = rec.elevation; }

            pairs.push((xi, yi));
        }
    }

    if pairs.len() < 3 {
        eprintln!("Arc {}: insufficient valid points after filtering.", arc.sat_id);
        return Vec::new();
    }

    // --- NEW: sort by x (ascending) and keep y aligned ---
    pairs.sort_by(|a, b| {
        match (a.0.partial_cmp(&b.0), a.0.is_nan() || b.0.is_nan()) {
            (Some(ord), false) => ord,
            _ => Ordering::Equal, // treat NaNs (already filtered) or incomparable as equal
        }
    });

    // Split back into x and y vectors (aligned)
    let mut x: Vec<f64> = Vec::with_capacity(pairs.len());
    let mut y: Vec<f64> = Vec::with_capacity(pairs.len());
    for (xi, yi) in pairs {
        x.push(xi);
        y.push(yi);
    }

    // Span from sorted x
    let min_x = *x.first().unwrap();
    let max_x = *x.last().unwrap();
    let w = max_x - min_x;
    if !w.is_finite() || w <= 1e-9 {
        eprintln!("Arc {}: span w too small ({:.3e}), skipping.", arc.sat_id, w);
        return Vec::new();
    }

    // Conservative frequency grid
    let cpw = 1.0 / w; 
    let ofac = cpw / 0.005; 
    let fc = n as f64 / (2.0 * w);
    let hifac = 15.0 / fc;
    let nout_theoretical = (0.5 * ofac * hifac * (x.len() as f64)).ceil() as usize;
    let nout = nout_theoretical.min(MAX_NOUT).max(2);

    // Period domain for this LS implementation
    // let pstart = 1.0 / (w * ofac);
    let pstart = 6.0;
    // let pstop  = hifac * (x.len() as f64) / (2.0 * w);
    let pstop = 16.0;
    // if !(pstart.is_finite() && pstop.is_finite()) || pstop <= pstart {
    //     eprintln!("Arc {}: invalid period range [{:.3e}, {:.3e}].", arc.sat_id, pstart, pstop);
    //     return Vec::new();
    // }

    let dp = (pstop - pstart) / (nout as f64 - 1.0);
    let mut pd: Vec<f64> = Vec::with_capacity(nout);
    let mut p = pstart;
    for _ in 0..nout {
        pd.push(p);
        p += dp;
    }

    println!(
        "Arc {}: n={} elev [{:.1}, {:.1}] x [{:.5}, {:.5}] w={:.5} ofac={:.1} hifac={:.1} n_freq={} (from={}, to={})",
        arc.sat_id, x.len(), min_elev, max_elev, min_x, max_x, w, ofac, hifac, nout, pstart, pstop
    );

    // Lomb–Scargle on sorted/paired data
    let (freqs, power) = match scirs2_signal::lombscargle::lombscargle(
        &x,
        &y,
        Some(&pd),
        Some("standard"),
        Some(true),
        Some(true),
        None,
        None,
    ) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Arc {}: lombscargle error: {}", arc.sat_id, e);
            return Vec::new();
        }
    };

    // Collect results into Vec<(frequency, power)>
    let results: Vec<(f64, f64)> = freqs.into_iter().zip(power.into_iter()).collect();

    results
}
