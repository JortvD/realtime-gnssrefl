use std::collections::HashMap;

use crate::{math::lombscargle, types::{Arc, Config, Record}};

pub fn find_arcs(records: &Vec<Record>) -> Vec<Arc> {
    let n_records = records.len();
    if n_records == 0 {
        return Vec::new();
    }

    // Group records by ID
    let mut by_id: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, rec) in records.iter().enumerate() {
        by_id.entry(rec.id).or_default().push(i);
    }

    let mut arcs = Vec::new();

    for (id, idxs) in by_id {
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

            if t - last_time > 120 && current_arc_indices.len() > 1 {
                // finalize previous arc
                let arc_indices = std::mem::take(&mut current_arc_indices);
                println!("Adding arc for ID {}: {} records from {} to {}", id, arc_indices.len(), arc_start_time, last_time);
                arcs.push(Arc {
                    sat_id: id,
                    time_start: arc_start_time,
                    time_end: last_time,
                    record_indices: arc_indices
                });

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
            arcs.push(Arc {
                sat_id: id,
                time_start: arc_start_time,
                time_end: last_time,
                record_indices: arc_indices
            });
        }
    }

    arcs
}

use polyfit_rs::polyfit_rs::polyfit;

pub fn fix_arc_elev_azim(arc: &Arc, records: &mut Vec<Record>) {
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

pub fn correct_arc_snr(arc: &Arc, records: &mut Vec<Record>) {
    // Collect SNR values for the arc's records
    // let snr_values: Vec<f64> = arc
    //     .record_indices
    //     .iter()
    //     .filter_map(|&idx| records.get(idx).map(|rec| rec.snr))
    //     .collect();

    // Detrend SNR using a 3rd order polynomial
    // let detrended = match scirs2_signal::detrend::detrend_poly(&snr_values, 3) {
    //     Ok(d) => d,
    //     Err(e) => {
    //         eprintln!("Detrending error for arc with sat_id {}: {}", arc.sat_id, e);
    //         return;
    //     }
    // };

    // Write detrended SNR back to the records
    // for (i, &idx) in arc.record_indices.iter().enumerate() {
    //     if let Some(rec) = records.get_mut(idx) {
    //         rec.snr = detrended[i];
    //     }
    // }
}

pub fn lin_range(start: f64, stop: f64, step_size: f64) -> Vec<f64> {
    let n_steps = ((stop - start) / step_size).ceil() as usize;
    let mut values = Vec::with_capacity(n_steps);
    let mut val = start;
    for _ in 0..n_steps {
        values.push(val);
        val += step_size;
    }
    values
}

pub fn find_arc_rh(arc: &Arc, records: &Vec<Record>, config: &Config) -> Vec<(f64, f64)> {
    let n = arc.record_indices.len();
    if n < 3 {
        eprintln!("Arc {}: too few points (n={}), skipping.", arc.sat_id, n);
        return Vec::new();
    }
    let steps = lin_range(config.min_height, config.max_height, config.step_size);

    let l1_wv_m = 299_792_458.0 / (1575.42e6); // L1 wavelength (m)
    let cf = l1_wv_m / 2.0;

    let arc_records: Vec<&Record> = arc.record_indices.iter().filter_map(|&idx| records.get(idx)).collect();
    let mut pairs: Vec<(f64, f64)> = arc_records.iter().map(|rec| ((rec.elevation.to_radians()).sin() / cf, rec.snr)).collect();

    // Sort pairs by elevation
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let (x, y): (Vec<f64>, Vec<f64>) = pairs.into_iter().unzip();

    // Lomb–Scargle on sorted/paired data
    let power = lombscargle(
        &x,
        &y,
        &steps,
    );

    steps.into_iter().zip(power.into_iter()).collect()
}

pub fn find_max_amplitude(rhs: &Vec<(f64, f64)>) -> Option<(f64, f64)> {
    rhs.iter().cloned().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
}