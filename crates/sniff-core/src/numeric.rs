//! Audited numeric conversions.
//!
//! `clippy::as_conversions` and the `cast_*` family are denied workspace-wide.
//! The only conversions that genuinely need `as` are float → integer (Rust has
//! no `TryFrom<f64>`); they live here, behind small functions whose contract
//! makes the result deterministic and explicitly saturating. Everything else
//! uses `From`/`TryFrom`.

/// `2^64`: the smallest `f64` at or above `u64::MAX`.
const U64_CEIL: f64 = 18_446_744_073_709_551_616.0;

/// Rounds an `f64` to the nearest `u32`, saturating.
///
/// `NaN` → `0`, negatives → `0`, values above `u32::MAX` → `u32::MAX`.
/// Intended for percentages (0–100) and pixel counts derived from
/// measurements, where the input is finite in practice.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "único ponto de `f64 → u32`; `.round()` + `clamp` tornam a conversão determinística e saturante"
)]
pub fn f64_to_u32(v: f64) -> u32 {
    v.round().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// Rounds an `f64` to the nearest `u64`, saturating.
///
/// `NaN` → `0`, negatives → `0`, values at or above `2^64` → `u64::MAX`.
/// Intended for CSS-pixel distances and durations derived from `f64` math.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "único ponto de `f64 → u64`; `.round()` + `clamp` tornam a conversão determinística e saturante"
)]
pub fn f64_to_u64(v: f64) -> u64 {
    v.round().clamp(0.0, U64_CEIL) as u64
}
