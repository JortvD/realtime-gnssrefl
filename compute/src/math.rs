// Enable Rayon only if you want parallelism; builds fine without it.
use rayon::prelude::*;

#[inline(always)]
fn kahan_add(sum: &mut f64, c: &mut f64, x: f64) {
    // Compensated summation to reduce cancellation error where it matters.
    let y = x - *c;
    let t = *sum + y;
    *c = (t - *sum) - y;
    *sum = t;
}

/// Lomb–Scargle periodogram (unweighted, Scargle 1982 normalization).
/// Faster: fewer trig calls, no needless clones, compact loops, optional parallelism.
/// 
/// - `x`: sample times
/// - `y`: observations
/// - `frequencies`: frequencies (cycles per unit of `x`)
///
/// Returns `power` aligned with `frequencies`.
pub fn lombscargle(x: &[f64], y: &[f64], frequencies: &[f64]) -> Vec<f64> {
    assert_eq!(x.len(), y.len(), "x and y must have the same length");

    // 1) Filter finite values and compute mean (Kahan).
    let mut xt = Vec::with_capacity(x.len());
    let mut yt = Vec::with_capacity(y.len());
    let (mut sum_y, mut comp_y) = (0.0_f64, 0.0_f64);

    for (&ti, &yi) in x.iter().zip(y.iter()) {
        if ti.is_finite() && yi.is_finite() {
            xt.push(ti);
            yt.push(yi);
            kahan_add(&mut sum_y, &mut comp_y, yi);
        }
    }

    if xt.is_empty() || frequencies.is_empty() {
        return vec![0.0; frequencies.len()];
    }

    let n = xt.len() as f64;
    let mean_y = sum_y / n;

    // 2) Mean-center in place.
    for v in &mut yt {
        *v -= mean_y;
    }

    // 3) Evaluate per frequency.
    let two_pi = core::f64::consts::PI * 2.0;
    let eps = 1e-15_f64;

    // Inner evaluator (serial). Uses only sin_cos(ωt) per pass (2 passes total).
    let eval_freq = |f: f64| -> f64 {
        if f.abs() < eps {
            return 0.0; // degenerate near-DC
        }
        let omega = two_pi * f;

        // ---- Pass 1: compute tau from sums of sin(2ωt), cos(2ωt)
        // Use identities: sin(2a) = 2 sin a cos a; cos(2a) = cos^2 a − sin^2 a
        let (mut s2, mut c2) = (0.0_f64, 0.0_f64);
        let (mut cs2, mut cc2) = (0.0_f64, 0.0_f64); // Kahan comps
        for &t in &xt {
            let (s, c) = (omega * t).sin_cos();
            // s2 += 2*s*c; c2 += c*c - s*s
            kahan_add(&mut s2, &mut cs2, 2.0_f64.mul_add(s * c, 0.0));
            kahan_add(&mut c2, &mut cc2, c * c - s * s);
        }
        // omega_tau = 0.5 * atan2(s2, c2)
        let omega_tau = 0.5 * s2.atan2(c2);
        let (s_tau, c_tau) = omega_tau.sin_cos();

        // ---- Pass 2: sums at shifted times using:
        // sin(ω(t-τ)) = sin(ωt) * c_tau - cos(ωt) * s_tau
        // cos(ω(t-τ)) = cos(ωt) * c_tau + sin(ωt) * s_tau
        let (mut yc, mut ys) = (0.0_f64, 0.0_f64);
        let (mut cyc, mut cys) = (0.0_f64, 0.0_f64); // Kahan comps for sensitive sums
        let (mut cc, mut ss) = (0.0_f64, 0.0_f64);   // non-negative accumulators → no Kahan

        // Iterate by index to help the compiler prove bounds once.
        for i in 0..xt.len() {
            let (s, c) = (omega * unsafe { *xt.get_unchecked(i) }).sin_cos();
            // rotate by τ
            let s_shift = s * c_tau - c * s_tau;
            let c_shift = c * c_tau + s * s_tau;

            let yv = unsafe { *yt.get_unchecked(i) };

            // yc += yv * c_shift; ys += yv * s_shift
            kahan_add(&mut yc, &mut cyc, yv * c_shift);
            kahan_add(&mut ys, &mut cys, yv * s_shift);

            // cc += c_shift*c_shift; ss += s_shift*s_shift  (use FMA when available)
            cc = c_shift.mul_add(c_shift, cc);
            ss = s_shift.mul_add(s_shift, ss);
        }

        let pc = if cc > eps { (yc * yc) / cc } else { 0.0 };
        let ps = if ss > eps { (ys * ys) / ss } else { 0.0 };
        0.5 * (pc + ps)
    };

    // Heuristic: small len → serial (cheaper), large → parallel.
    const PAR_THRESHOLD: usize = 256;
    if frequencies.len() < PAR_THRESHOLD {
        let mut power = Vec::with_capacity(frequencies.len());
        for &f in frequencies {
            power.push(eval_freq(f));
        }
        power
    } else {
        frequencies.par_iter().map(|&f| eval_freq(f)).collect()
    }
}
