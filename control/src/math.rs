use core::f32::consts::PI;

/// Kahan compensated addition (f32).
#[inline(always)]
fn kahan_add(sum: &mut f32, c: &mut f32, x: f32) {
    let y = x - *c;
    let t = *sum + y;
    *c = (t - *sum) - y;
    *sum = t;
}

/// Lomb–Scargle (unweighted, Scargle 1982), no_std, f32.
/// Drop-in optimized general version: packs valid samples, precomputes 2πt.
#[inline(always)]
pub fn lombscargle_no_std(
    x: &[f32],
    y: &[f32],
    frequencies: &[f32],
    power_out: &mut [f32],
) -> usize {
    let m = core::cmp::min(frequencies.len(), power_out.len());
    if m == 0 { return 0; }

    // Pass 0: mean over finite pairs
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
    if n == 0 {
        for d in &mut power_out[..m] { *d = 0.0; }
        return m;
    }
    let mean_y = sum_y / (n as f32);

    // Pack valid pairs + precompute phi = 2π t, and centered yv.
    // We reuse power_out's tail as a tiny temp when m < n? No—keep code simple and safe.
    // Ask caller for modest stack? We'll keep fixed on stack up to len via small rolling buffer:
    // Since we promised "no allocation", we do two passes per frequency if n is huge; still faster due to no finiteness branches.
    let mut count = 0usize;

    // To avoid dynamic alloc, we create small on-stack scratch windows. For MCUs, this is cheap.
    // We’ll pre-pack into fixed stack chunks and process chunk-by-chunk.
    const CHUNK: usize = 256; // tune to L1 & your cache/flash wait states
    let mut buf_phi = [0.0f32; CHUNK];
    let mut buf_yv  = [0.0f32; CHUNK];

    const TWO_PI: f32 = PI * 2.0;
    const EPS: f32 = 1.0e-7;

    // Indices of valid pairs (single pass materialization as we stream chunks)
    // We’ll rescan x/y to refill the chunk; avoids heap.
    // Helper closure to (re)fill a chunk starting at `start_idx`, returning (filled, next_start).
    #[inline(always)]
    fn refill_chunk(
        x: &[f32], y: &[f32], start: usize,
        buf_phi: &mut [f32; CHUNK],
        buf_yv:  &mut [f32; CHUNK],
        mean_y: f32
    ) -> (usize, usize) {
        let len = core::cmp::min(x.len(), y.len());
        let mut filled = 0usize;
        let mut i = start;
        while i < len && filled < CHUNK {
            let (ti, yi) = unsafe { (*x.get_unchecked(i), *y.get_unchecked(i)) };
            if ti.is_finite() && yi.is_finite() {
                buf_phi[filled] = TWO_PI * ti;
                buf_yv [filled] = yi - mean_y;
                filled += 1;
            }
            i += 1;
        }
        (filled, i)
    }

    // Evaluate each frequency
    for k in 0..m {
        let f = frequencies[k];
        if f.abs() < EPS {
            power_out[k] = 0.0;
            continue;
        }

        let omega  = TWO_PI * f;
        // Pass 1: tau via sums of sin(2ωt), cos(2ωt)
        let (mut s2, mut c2) = (0.0f32, 0.0f32);
        let (mut cs2, mut cc2) = (0.0f32, 0.0f32);

        let mut start = 0usize;
        loop {
            let (filled, next) = refill_chunk(x, y, start, &mut buf_phi, &mut buf_yv, mean_y);
            if filled == 0 { break; }
            // use phi (2πt) to save a mul
            for i in 0..filled {
                let a2 = (omega + omega) * (buf_phi[i] / TWO_PI); // a2 = 2ωt; but we only have φ=2πt
                // more efficient: a2 = (2ω/2π) * φ = (ω/π) * φ
                // But keep it numerically stable:
                let a2 = (omega / PI) * buf_phi[i];
                let (s_2a, c_2a) = libm::sincosf(a2);
                kahan_add(&mut s2, &mut cs2, s_2a);
                kahan_add(&mut c2, &mut cc2, c_2a);
            }
            start = next;
        }

        let omega_tau = 0.5 * libm::atan2f(s2, c2);
        let (s_tau, c_tau) = libm::sincosf(omega_tau);

        // Pass 2: accumulate at shifted phase using rotation
        let (mut yc, mut ys) = (0.0f32, 0.0f32);
        let (mut cyc, mut cys) = (0.0f32, 0.0f32);
        let (mut cc, mut ss)  = (0.0f32, 0.0f32);

        let mut start2 = 0usize;
        loop {
            let (filled, next) = refill_chunk(x, y, start2, &mut buf_phi, &mut buf_yv, mean_y);
            if filled == 0 { break; }
            for i in 0..filled {
                let a = (omega / TWO_PI) * buf_phi[i]; // a = ωt = (ω/2π) * (2πt)
                let (s, c) = libm::sincosf(a);

                let s_shift = s * c_tau - c * s_tau;
                let c_shift = c * c_tau + s * s_tau;

                let yv = buf_yv[i];
                kahan_add(&mut yc, &mut cyc, yv * c_shift);
                kahan_add(&mut ys, &mut cys, yv * s_shift);

                cc += c_shift * c_shift;
                ss += s_shift * s_shift;
            }
            start2 = next;
        }

        let pc = if cc > EPS { (yc * yc) / cc } else { 0.0 };
        let ps = if ss > EPS { (ys * ys) / ss } else { 0.0 };
        power_out[k] = 0.5 * (pc + ps);
    }

    m
}

use core::cmp::min;

use defmt::info;

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