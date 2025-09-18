use core::f32::consts::PI;

/// Kahan compensated addition (f32).
#[inline(always)]
fn kahan_add(sum: &mut f32, c: &mut f32, x: f32) {
    let y = x - *c;
    let t = *sum + y;
    *c = (t - *sum) - y;
    *sum = t;
}

/// Lomb–Scargle periodogram (unweighted, Scargle 1982 normalization), no_std, f32.
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
#[inline(always)]
pub fn lombscargle_no_std(
    x: &[f32],
    y: &[f32],
    frequencies: &[f32],
    power_out: &mut [f32],
) -> usize {
    let m = core::cmp::min(frequencies.len(), power_out.len());
    if m == 0 {
        return 0;
    }

    // --- Pass 0: mean of finite y's where (x,y) are both finite
    let (mut sum_y, mut comp_y) = (0.0_f32, 0.0_f32);
    let mut n: usize = 0;

    // manual indexing avoids iterator overhead in no_std
    let len = core::cmp::min(x.len(), y.len());
    for i in 0..len {
        let ti = x[i];
        let yi = y[i];
        if ti.is_finite() && yi.is_finite() {
            kahan_add(&mut sum_y, &mut comp_y, yi);
            n += 1;
        }
    }

    if n == 0 {
        // nothing to do; define output as zeros
        for dst in &mut power_out[..m] {
            *dst = 0.0;
        }
        return m;
    }

    let n_f = n as f32;
    let mean_y = sum_y / n_f;

    const TWO_PI: f32 = PI * 2.0;
    // EPS tuned for f32; keeps divisions stable without over-penalizing small bins
    const EPS: f32 = 1.0e-7;

    // --- Evaluate each frequency serially
    for k in 0..m {
        let f = frequencies[k];

        // Handle near-DC gracefully
        if f.abs() < EPS {
            power_out[k] = 0.0;
            continue;
        }

        let omega = TWO_PI * f;

        // ---- Pass 1: compute tau via sums of sin(2ωt), cos(2ωt)
        let (mut s2, mut c2) = (0.0_f32, 0.0_f32);
        let (mut cs2, mut cc2) = (0.0_f32, 0.0_f32); // Kahan comps

        for i in 0..len {
            let ti = x[i];
            let yi = y[i];
            if !(ti.is_finite() && yi.is_finite()) {
                continue;
            }
            let a = omega * ti;

            // libm for no_std trig
            let s = libm::sinf(a);
            let c = libm::cosf(a);

            // sin(2a) = 2 sin a cos a ; cos(2a) = cos^2 a − sin^2 a
            kahan_add(&mut s2, &mut cs2, 2.0 * s * c);
            kahan_add(&mut c2, &mut cc2, c * c - s * s);
        }

        // omega_tau = 0.5 * atan2(s2, c2)
        let omega_tau = 0.5 * libm::atan2f(s2, c2);
        let s_tau = libm::sinf(omega_tau);
        let c_tau = libm::cosf(omega_tau);

        // ---- Pass 2: sums at shifted times, mean-centering y on the fly
        let (mut yc, mut ys) = (0.0_f32, 0.0_f32);
        let (mut cyc, mut cys) = (0.0_f32, 0.0_f32); // Kahan comps (sensitive)
        let (mut cc, mut ss) = (0.0_f32, 0.0_f32);   // non-negative; use mul_add

        for i in 0..len {
            let ti = x[i];
            let yi = y[i];
            if !(ti.is_finite() && yi.is_finite()) {
                continue;
            }
            let yv = yi - mean_y;

            let a = omega * ti;
            let s = libm::sinf(a);
            let c = libm::cosf(a);

            // rotate by τ
            let s_shift = s * c_tau - c * s_tau;
            let c_shift = c * c_tau + s * s_tau;

            kahan_add(&mut yc, &mut cyc, yv * c_shift);
            kahan_add(&mut ys, &mut cys, yv * s_shift);

            // Use mul_add to leverage FMA when available (or emulate otherwise).
            cc += c_shift * c_shift; 
            ss += s_shift * s_shift;
        }

        let pc = if cc > EPS { (yc * yc) / cc } else { 0.0 };
        let ps = if ss > EPS { (ys * ys) / ss } else { 0.0 };
        power_out[k] = 0.5 * (pc + ps);
    }

    m
}

