//! Numeric helpers that reproduce JavaScript behaviour where Rust's
//! standard library differs, so the port tracks the browser numerically.

/// `Math.hypot(x, y)` as V8 computes it: scale by the larger magnitude and
/// sum the squares with Kahan compensation, then take the root. Rust's
/// `f64::hypot` calls the C runtime, which can round differently in the
/// last place; over thousands of rope and collision steps such differences
/// grow, so the simulation uses this everywhere the browser uses Math.hypot.
pub fn hypot(x: f64, y: f64) -> f64 {
    let (ax, ay) = (x.abs(), y.abs());
    if ax.is_infinite() || ay.is_infinite() {
        return f64::INFINITY;
    }
    if ax.is_nan() || ay.is_nan() {
        return f64::NAN;
    }
    let max = ax.max(ay);
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for v in [ax, ay] {
        let n = v / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// Distance between two points.
pub fn dist(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    hypot(ax - bx, ay - by)
}

/// `Math.round`: halves round toward +infinity (Rust's `round` goes away from zero).
pub fn round(v: f64) -> f64 {
    (v + 0.5).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypot_basics() {
        assert_eq!(hypot(3.0, 4.0), 5.0);
        assert_eq!(hypot(0.0, 0.0), 0.0);
        assert_eq!(hypot(-5.0, 0.0), 5.0);
    }

    #[test]
    fn round_matches_js_for_negative_halves() {
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(2.5), 3.0);
    }
}
