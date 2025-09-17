use core::f64::consts::PI;

/// Kahan compensated addition (f64).
#[inline(always)]
fn kahan_add(sum: &mut f64, c: &mut f64, x: f64) {
    let y = x - *c;
    let t = *sum + y;
    *c = (t - *sum) - y;
    *sum = t;
}

/// Lomb–Scargle periodogram (unweighted, Scargle 1982 normalization) for `no_std`.
///
/// - `x`: sample times
/// - `y`: observations (same length as `x`)
/// - `frequencies`: frequencies to evaluate (cycles per unit of `x`)
/// - `power_out`: output slice to receive power values; only the first
///   `min(frequencies.len(), power_out.len())` entries are written.
///
/// Skips non-finite `(x, y)` pairs. No allocation. Serial only (single core).
///
/// Returns the number of power values written.
pub fn lombscargle_no_std(
    x: &[f64],
    y: &[f64],
    frequencies: &[f64],
    power_out: &mut [f64],
) -> usize {
    let m = core::cmp::min(frequencies.len(), power_out.len());
    if m == 0 {
        return 0;
    }

    // --- Pass 0: mean of finite y's where (x,y) are both finite
    let (mut sum_y, mut comp_y) = (0.0_f64, 0.0_f64);
    let mut n: u64 = 0;
    for (&ti, &yi) in x.iter().zip(y.iter()) {
        if ti.is_finite() && yi.is_finite() {
            kahan_add(&mut sum_y, &mut comp_y, yi);
            n += 1;
        }
    }

    if n == 0 {
        // nothing to do; define output as zeros
        for p in &mut power_out[..m] {
            *p = 0.0;
        }
        return m;
    }

    let n_f = n as f64;
    let mean_y = sum_y / n_f;

    const TWO_PI: f64 = PI * 2.0;
    const EPS: f64 = 1e-15;

    // --- Evaluate each frequency serially
    for (dst, &f) in power_out.iter_mut().zip(&frequencies[..m]) {
        // Handle near-DC gracefully
        if f.abs() < EPS {
            *dst = 0.0;
            continue;
        }

        let omega = TWO_PI * f;

        // ---- Pass 1: compute tau via sums of sin(2ωt), cos(2ωt)
        let (mut s2, mut c2) = (0.0_f64, 0.0_f64);
        let (mut cs2, mut cc2) = (0.0_f64, 0.0_f64); // Kahan comps

        for (&ti, &yi) in x.iter().zip(y.iter()) {
            if !(ti.is_finite() && yi.is_finite()) {
                continue;
            }
            let a = omega * ti;

            // use libm for no_std trig
            let s = libm::sin(a);
            let c = libm::cos(a);

            // sin(2a) = 2 sin a cos a ; cos(2a) = cos^2 a − sin^2 a
            kahan_add(&mut s2, &mut cs2, 2.0 * s * c);
            kahan_add(&mut c2, &mut cc2, c * c - s * s);
        }

        // omega_tau = 0.5 * atan2(s2, c2)
        let omega_tau = 0.5 * libm::atan2(s2, c2);
        let s_tau = libm::sin(omega_tau);
        let c_tau = libm::cos(omega_tau);

        // ---- Pass 2: sums at shifted times, mean-centering y on the fly
        let (mut yc, mut ys) = (0.0_f64, 0.0_f64);
        let (mut cyc, mut cys) = (0.0_f64, 0.0_f64); // Kahan comps (sensitive)
        let (mut cc, mut ss) = (0.0_f64, 0.0_f64);   // non-negative → no Kahan

        for (&ti, &yi) in x.iter().zip(y.iter()) {
            if !(ti.is_finite() && yi.is_finite()) {
                continue;
            }
            let yv = yi - mean_y;

            let a = omega * ti;
            let s = libm::sin(a);
            let c = libm::cos(a);

            // rotate by τ
            let s_shift = s * c_tau - c * s_tau;
            let c_shift = c * c_tau + s * s_tau;

            kahan_add(&mut yc, &mut cyc, yv * c_shift);
            kahan_add(&mut ys, &mut cys, yv * s_shift);

            // FMA not available in software; regular multiply-add is fine.
            cc += c_shift * c_shift;
            ss += s_shift * s_shift;
        }

        let pc = if cc > EPS { (yc * yc) / cc } else { 0.0 };
        let ps = if ss > EPS { (ys * ys) / ss } else { 0.0 };
        *dst = 0.5 * (pc + ps);
    }

    m
}
