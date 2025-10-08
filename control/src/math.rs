use core::f32::consts::PI;

/// Kahan compensated addition (f32).
#[inline(always)]
fn kahan_add(sum: &mut f32, c: &mut f32, x: f32) {
    let y = x - *c;
    let t = *sum + y;
    *c = (t - *sum) - y;
    *sum = t;
}

// -------- Fixed-point helpers (i16 <-> [-1, 1]) --------

#[inline(always)]
fn q_from_f32(x: f32) -> i16 {
    // clamp to [-1,1], then map to i16 range [-32767, 32767]
    const SCALE: f32 = 32767.0;
    let y = if x > 1.0 { 1.0 } else if x < -1.0 { -1.0 } else { x };
    (y * SCALE) as i16
}

#[inline(always)]
fn f32_from_q(q: i16) -> f32 {
    const INV_SCALE: f32 = 1.0 / 32767.0;
    (q as f32) * INV_SCALE
}

/// Scratch buffers for the uniform-grid accelerator, memory-reduced.
/// All slices must have length >= number of valid (finite) samples.
pub struct LsScratch<'a, const CAP: usize> {
    /// packed (y - mean), kept in f32 for accuracy in accumulations
    pub yv:  &'a mut [f32; CAP],
    /// sin(ω_k t_i) current, quantized
    pub s_w: &'a mut [i16; CAP],
    /// cos(ω_k t_i) current, quantized
    pub c_w: &'a mut [i16; CAP],
    /// sin(Δω t_i), constant across k, quantized
    pub s_d: &'a mut [i16; CAP],
    /// cos(Δω t_i), constant across k, quantized
    pub c_d: &'a mut [i16; CAP],
}

#[inline(always)]
fn pack_center_and_init<const CAP: usize>(
    x: &[f32],
    y: &[f32],
    f0_omega: f32,
    d_omega: f32,
    sc: &mut LsScratch<'_, CAP>,
) -> usize {
    // mean over finite
    let len = core::cmp::min(x.len(), y.len());
    let (mut sum_y, mut comp_y) = (0.0f32, 0.0f32);
    let mut n = 0usize;
    for i in 0..len {
        let (ti, yi) = unsafe { (*x.get_unchecked(i), *y.get_unchecked(i)) };
        if ti.is_finite() && yi.is_finite() {
            kahan_add(&mut sum_y, &mut comp_y, yi);
            n += 1;
        }
    }
    if n == 0 { return 0; }
    let mean_y = sum_y / (n as f32);

    // pack & precompute sin/cos at ω0 t and Δω t (quantized)
    let mut j = 0usize;
    for i in 0..len {
        let (ti, yi) = unsafe { (*x.get_unchecked(i), *y.get_unchecked(i)) };
        if !(ti.is_finite() && yi.is_finite()) { continue; }

        sc.yv[j] = yi - mean_y;

        let a0 = f0_omega * ti;
        let (s0, c0) = libm::sincosf(a0);
        sc.s_w[j] = q_from_f32(s0);
        sc.c_w[j] = q_from_f32(c0);

        let d = d_omega * ti;
        let (sd, cd) = libm::sincosf(d);
        sc.s_d[j] = q_from_f32(sd);
        sc.c_d[j] = q_from_f32(cd);

        j += 1;
        if j == CAP { break; } // hard cap safety
    }
    j
}

/// Ultra-fast Lomb–Scargle for **uniform frequency grid**:
/// frequencies: f_k = f0 + k*df, for k in [0, m).
/// Writes min(m, power_out.len()) powers.
/// Returns number written.
///
/// Memory-optimized version:
/// - stores sin/cos tables as i16 fixed-point
/// - drops unused packed time buffer
#[inline(always)]
pub async fn lombscargle_no_std<const CAP: usize>(
    x: &[f32],
    y: &[f32],
    f0: f32,
    df: f32,
    m: usize,
    power_out: &mut [f32],
) -> usize {
    let m = core::cmp::min(m, power_out.len());
    if m == 0 { return 0; }

    const TWO_PI: f32 = PI * 2.0;
    const EPS: f32 = 1.0e-7;

    let omega0 = TWO_PI * f0;
    let domega = TWO_PI * df;

     // Use stack-allocated arrays instead of mutable statics.
    let mut yv:  [f32; CAP] = [0.0; CAP];
    let mut sw:  [i16; CAP] = [0; CAP];
    let mut cw:  [i16; CAP] = [0; CAP];
    let mut sd:  [i16; CAP] = [0; CAP];
    let mut cd:  [i16; CAP] = [0; CAP];

    // In your task (single-threaded over the buffers):
    let mut sc = LsScratch {
        yv:  &mut yv,
        s_w: &mut sw,
        c_w: &mut cw,
        s_d: &mut sd,
        c_d: &mut cd,
    };

    // Pack/center + init quantized sin/cos tables
    let n = pack_center_and_init(x, y, omega0, domega, &mut sc);
    if n == 0 {
        for d in &mut power_out[..m] { *d = 0.0; }
        return m;
    }

    // For each frequency k
    for k in 0..m {
        // --- Pass 1: tau from current sin(ωt), cos(ωt)
        let (mut s2, mut c2) = (0.0f32, 0.0f32);
        let (mut cs2, mut cc2) = (0.0f32, 0.0f32);
        // sin(2a)=2sc; cos(2a)=c^2-s^2
        for i in 0..n {
            let s = f32_from_q(unsafe { *sc.s_w.get_unchecked(i) });
            let c = f32_from_q(unsafe { *sc.c_w.get_unchecked(i) });
            kahan_add(&mut s2, &mut cs2, 2.0 * s * c);
            kahan_add(&mut c2, &mut cc2, c * c - s * s);
        }
        let omega_tau = 0.5 * libm::atan2f(s2, c2);
        let (s_tau, c_tau) = libm::sincosf(omega_tau);

        // --- Pass 2: rotate by τ, accumulate
        let (mut yc, mut ys) = (0.0f32, 0.0f32);
        let (mut cyc, mut cys) = (0.0f32, 0.0f32);
        let (mut cc, mut ss)  = (0.0f32, 0.0f32);

        for i in 0..n {
            let s = f32_from_q(unsafe { *sc.s_w.get_unchecked(i) });
            let c = f32_from_q(unsafe { *sc.c_w.get_unchecked(i) });

            // rotate (s,c) -> (s_shift, c_shift)
            let s_shift = s * c_tau - c * s_tau;
            let c_shift = c * c_tau + s * s_tau;

            let yv = unsafe { *sc.yv.get_unchecked(i) };
            kahan_add(&mut yc, &mut cyc, yv * c_shift);
            kahan_add(&mut ys, &mut cys, yv * s_shift);

            cc += c_shift * c_shift;
            ss += s_shift * s_shift;
        }

        let pc = if cc > EPS { (yc * yc) / cc } else { 0.0 };
        let ps = if ss > EPS { (ys * ys) / ss } else { 0.0 };
        power_out[k] = 0.5 * (pc + ps);

        // --- Advance sin/cos to next frequency using angle addition with Δω
        // sin' = s*cΔ + c*sΔ ; cos' = c*cΔ - s*sΔ
        if k + 1 < m {
            for i in 0..n {
                let s  = f32_from_q(unsafe { *sc.s_w.get_unchecked(i) });
                let c  = f32_from_q(unsafe { *sc.c_w.get_unchecked(i) });
                let sd = f32_from_q(unsafe { *sc.s_d.get_unchecked(i) });
                let cd = f32_from_q(unsafe { *sc.c_d.get_unchecked(i) });

                let s_next = s * cd + c * sd;
                let c_next = c * cd - s * sd;

                unsafe {
                    *sc.s_w.get_unchecked_mut(i) = q_from_f32(s_next);
                    *sc.c_w.get_unchecked_mut(i) = q_from_f32(c_next);
                }
            }
        }

        yield_now().await;
    }

    m
}



use core::cmp::min;

/// Solve A x = b in-place via Gaussian elimination with partial pivoting (no_std, f32).
/// - `a` is an NxN matrix (only top-left n×n is used)
/// - `b` is length N (only first n used)
/// Returns `true` on success, `false` if a near-singular pivot is found.
pub fn gauss_solve<const N: usize>(a: &mut [[f32; N]; N], b: &mut [f32; N], n: usize) -> bool {
    const EPS: f32 = 1e-6;

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

pub const DEG: usize = 3; // polynomial degree for polyfit_and_smooth_no_std

/// Fit a polynomial of degree `DEG` to (x,y) via least squares and overwrite `y` with
/// the fitted values at each `x[i]`.
///
/// - `DEG` is the (compile-time) polynomial degree (e.g., 1=line, 2=quadratic).
/// - Works in `no_std`, single-core, no allocation, `f32`.
/// - Non-finite (x,y) pairs are **ignored** during fitting.
/// - On write-back, only entries with finite `x[i]` are updated; others are left unchanged.
/// - If there are fewer than `DEG+1` valid points, it automatically downgrades to
///   `effective_deg = valid_points-1`.
///
/// Returns the degree actually used (effective degree).
pub fn polyfit_and_smooth_no_std(x: &[f32], y: &mut [f32]) -> usize {
    debug_assert_eq!(x.len(), y.len());

    // Count valid pairs and collect power sums up to 2*DEG without pow()
    // s[k] = sum t^k  for k=0..2*DEG
    // bt[i] = sum y * t^i for i=0..DEG
    let mut valid = 0usize;

    // Accumulators (+ compensation) sized for DEG
    let mut s:   [f32; 2 * DEG + 1] = [0.0; 2 * DEG + 1];
    let mut cs:  [f32; 2 * DEG + 1] = [0.0; 2 * DEG + 1];
    let mut bt:  [f32; DEG + 1]     = [0.0; DEG + 1];
    let mut cbt: [f32; DEG + 1]     = [0.0; DEG + 1];

    for (&xi, &yi) in x.iter().zip(y.iter()) {
        if !(xi.is_finite() && yi.is_finite()) {
            continue;
        }
        valid += 1;

        // iteratively build powers of xi into s
        let mut p = 1.0_f32;
        for k in 0..(2 * DEG + 1) {
            kahan_add(&mut s[k], &mut cs[k], p);
            p *= xi;
        }

        // accumulate y * xi^i into bt
        let mut p2 = 1.0_f32;
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
    let eff_deg = min(DEG, valid.saturating_sub(1));

    // Constant fit fast-path
    if eff_deg == 0 {
        // s[0] accumulated 1 per valid sample => count
        let denom = if s[0] > 0.0 { s[0] } else { 1.0 };
        let a0 = bt[0] / denom;
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
    let mut a: [[f32; DEG + 1]; DEG + 1] = [[0.0; DEG + 1]; DEG + 1];
    let mut bvec: [f32; DEG + 1] = [0.0; DEG + 1];

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
        let denom = if s[0] > 0.0 { s[0] } else { 1.0 };
        let a0 = bt[0] / denom;
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


use core::cmp::Ordering;

use embassy_futures::yield_now;

#[inline]
fn lt(a: f32, b: f32) -> bool {
    a.total_cmp(&b) == Ordering::Less
}

fn insertion_sort_xy(x: &mut [f32], y: &mut [f32]) {
    for i in 1..x.len() {
        let mut j = i;
        while j > 0 && lt(x[j], x[j - 1]) {
            x.swap(j, j - 1);
            y.swap(j, j - 1);
            j -= 1;
        }
    }
}

fn median3_idx(x: &[f32], a: usize, b: usize, c: usize) -> usize {
    let (va, vb, vc) = (x[a], x[b], x[c]);
    if lt(va, vb) {
        if lt(vb, vc) { b } else if lt(va, vc) { c } else { a }
    } else {
        if lt(va, vc) { a } else if lt(vb, vc) { c } else { b }
    }
}

fn partition_xy(x: &mut [f32], y: &mut [f32], pivot_idx: usize) -> usize {
    let len = x.len();
    x.swap(pivot_idx, len - 1);
    y.swap(pivot_idx, len - 1);
    let pivot = x[len - 1];

    let mut store = 0;
    for i in 0..(len - 1) {
        if lt(x[i], pivot) {
            x.swap(i, store);
            y.swap(i, store);
            store += 1;
        }
    }
    x.swap(store, len - 1);
    y.swap(store, len - 1);
    store
}

pub fn quicksort_xy(x: &mut [f32], y: &mut [f32]) {
    const INSERTION_THRESHOLD: usize = 16;

    let len = x.len();
    if len <= 1 { return; }
    if len <= INSERTION_THRESHOLD {
        insertion_sort_xy(x, y);
        return;
    }

    // Median-of-three pivot (first, middle, last)
    let pidx = median3_idx(x, 0, len / 2, len - 1);
    let pivot_at = partition_xy(x, y, pidx);

    // Recurse on left and right (skip pivot)
    let (xl, xr) = x.split_at_mut(pivot_at);
    let (yl, yr) = y.split_at_mut(pivot_at);

    quicksort_xy(xl, yl);
    if xr.len() > 1 {
        quicksort_xy(&mut xr[1..], &mut yr[1..]);
    }
}