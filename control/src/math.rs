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

/// Solve A x = b in-place via Gaussian elimination with partial pivoting.
/// - `a` is an NxN matrix (only top-left n×n is used)
/// - `b` is length N (only first n used)
/// Returns `true` on success, `false` if a near-singular pivot is found.
fn gauss_solve<const N: usize>(a: &mut [[f64; N]; N], b: &mut [f64; N], n: usize) -> bool {
    const EPS: f64 = 1e-12;

    // Forward elimination
    for k in 0..n {
        // pivot row r with max |a[r][k]|
        let mut r = k;
        let mut maxv = a[k][k].abs();
        for i in (k + 1)..n {
            let v = a[i][k].abs();
            if v > maxv {
                maxv = v;
                r = i;
            }
        }
        if maxv < EPS {
            return false; // singular/ill-conditioned
        }
        if r != k {
            // swap rows k <-> r
            for j in k..n {
                let tmp = a[k][j];
                a[k][j] = a[r][j];
                a[r][j] = tmp;
            }
            let tb = b[k];
            b[k] = b[r];
            b[r] = tb;
        }

        // eliminate
        let pivot = a[k][k];
        for i in (k + 1)..n {
            let f = a[i][k] / pivot;
            // subtract f * row k from row i
            for j in (k + 1)..n {
                a[i][j] -= f * a[k][j];
            }
            a[i][k] = 0.0;
            b[i] -= f * b[k];
        }
    }

    // Back substitution
    for i in (0..n).rev() {
        let mut s = b[i];
        for j in (i + 1)..n {
            s -= a[i][j] * b[j];
        }
        let piv = a[i][i];
        if piv.abs() < EPS {
            return false;
        }
        b[i] = s / piv;
    }
    true
}

const DEG: usize = 3; // polynomial degree for polyfit_and_smooth_no_std

/// Fit a polynomial of degree `DEG` to (x,y) via least squares and overwrite `y` with
/// the fitted values at each `x[i]`.
///
/// - `DEG` is the (compile-time) polynomial degree (e.g., 1=line, 2=quadratic).
/// - Works in `no_std`, single-core, no allocation.
/// - Non-finite (x,y) pairs are **ignored** during fitting.
/// - On write-back, only entries with finite `x[i]` are updated; others are left unchanged.
/// - If there are fewer than `DEG+1` valid points, it automatically downgrades to
///   `effective_deg = valid_points-1`.
///
/// Returns the degree actually used (effective degree).
pub fn polyfit_and_smooth_no_std(x: &[f64], y: &mut [f64]) -> usize {
    debug_assert_eq!(x.len(), y.len());
    let n_pts = x.len();

    // Count valid pairs and collect power sums up to 2*DEG without pow()
    // s[k] = sum t^k  for k=0..2*DEG
    // b[i] = sum y * t^i for i=0..DEG
    let mut valid = 0usize;

    // Use only the needed lengths; cap degree by the number of valid pairs (later).
    let mut s: [f64; 2 * DEG + 1] = [0.0; 2 * DEG + 1];
    let mut cs: [f64; 2 * DEG + 1] = [0.0; 2 * DEG + 1];
    let mut bt: [f64; DEG + 1] = [0.0; DEG + 1];
    let mut cbt: [f64; DEG + 1] = [0.0; DEG + 1];

    for (&xi, &yi) in x.iter().zip(y.iter()) {
        if !(xi.is_finite() && yi.is_finite()) {
            continue;
        }
        valid += 1;

        // iteratively build powers of xi
        let mut p = 1.0_f64;
        for k in 0..(2 * DEG + 1) {
            kahan_add(&mut s[k], &mut cs[k], p);
            p *= xi;
        }

        // reset p and accumulate y * xi^i
        let mut p2 = 1.0_f64;
        for i in 0..(DEG + 1) {
            kahan_add(&mut bt[i], &mut cbt[i], yi * p2);
            p2 *= xi;
        }
    }

    if valid == 0 {
        // nothing to fit; leave y unchanged and report 0-degree used
        return 0;
    }

    // Effective degree cannot exceed valid-1
    let mut eff_deg = DEG.min(valid.saturating_sub(1));
    // Safety: cap at array limits (already ensured by consts), but handle degenerate sums
    if eff_deg == 0 {
        // Constant fit: a0 = mean(y on valid)
        let a0 = bt[0] / (s[0].max(1.0));
        for (xi, yi) in x.iter().zip(y.iter_mut()) {
            if xi.is_finite() {
                *yi = a0;
            }
        }
        return 0;
    }

    let n = eff_deg + 1;

    // Build normal-equations matrix A and rhs b for size n
    // A[i][j] = sum x^(i+j) = s[i+j],  b[i] = sum y x^i = bt[i]
    let mut a: [[f64; DEG + 1]; DEG + 1] = [[0.0; DEG + 1]; DEG + 1];
    let mut bvec: [f64; DEG + 1] = [0.0; DEG + 1];

    for i in 0..n {
        bvec[i] = bt[i];
        for j in 0..n {
            a[i][j] = s[i + j];
        }
    }

    // Solve for coefficients in-place (solution written into bvec[0..n])
    let ok = gauss_solve::<{ DEG + 1 }>(&mut a, &mut bvec, n);
    if !ok {
        // fall back: constant fit to mean of y over valid samples
        let a0 = bt[0] / (s[0].max(1.0));
        for (xi, yi) in x.iter().zip(y.iter_mut()) {
            if xi.is_finite() {
                *yi = a0;
            }
        }
        return 0;
    }

    // Evaluate fitted polynomial at each x and overwrite y
    for (xi, yi) in x.iter().zip(y.iter_mut()) {
        if !xi.is_finite() {
            continue;
        }
        // Horner's method
        let mut acc = bvec[n - 1];
        for k in (0..(n - 1)).rev() {
            acc = acc * *xi + bvec[k];
        }
        *yi = acc;
    }

    eff_deg
}

/* -------------------------
   Optional: f32 variant
   -------------------------
   Switch types to f32, constants to f32, and adjust EPS in gauss_solve
   if you want better speed on RP2040 (no FPU).
*/
