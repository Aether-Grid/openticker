/// Converts a `usize` to `f64` (exact for values up to 2^53).
///
/// `f64` has a 53-bit significand, so all `usize` values up to 2^53
/// (`9_007_199_254_740_992`) round-trip without loss. On any realistic platform
/// `usize` is at most 64 bits, but even a 64-bit `usize` only loses precision
/// for values above 2^53 — far beyond any realistic indicator window or index.
///
/// This replaces the previous `u32::try_from(value).unwrap_or(u32::MAX)` pattern
/// that silently clamped values ≥ 2^32, producing incorrect results for very
/// large indices (e.g. long trade durations counted in ticks).
#[allow(clippy::cast_precision_loss)]
#[inline]
pub fn usize_to_f64(value: usize) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn usize_to_f64_exact_above_u32_max() {
        // The old u32::try_from path clamped this to u32::MAX (4_294_967_295.0).
        // The correct result is 4_294_967_296.0.
        let value = (u32::MAX as usize) + 1;
        assert_eq!(usize_to_f64(value), 4_294_967_296.0_f64);
    }

    #[test]
    fn usize_to_f64_zero() {
        assert_eq!(usize_to_f64(0), 0.0_f64);
    }

    #[test]
    fn usize_to_f64_small_values() {
        assert_eq!(usize_to_f64(1), 1.0_f64);
        assert_eq!(usize_to_f64(200), 200.0_f64);
    }

    #[test]
    fn usize_to_f64_u32_max_itself() {
        // u32::MAX should still be exact (it is well within 2^53).
        assert_eq!(usize_to_f64(u32::MAX as usize), f64::from(u32::MAX));
    }
}
