//! Typed accessors for telemetry-parser [`TagValue`] variants.
//!
//! Telemetry-parser exposes camera metadata as a heterogenous enum covering a
//! dozen numeric primitives (`u8`, `i16`, `u32`, `f32`, …), vectors, strings,
//! and nested JSON blobs. Downstream extraction code should not need to match
//! on every variant of `TagValue` just to read an ISO value or a GPS scale
//! vector. These accessors collapse the enum into a single `Option<f64>` (or
//! `Option<Vec<f64>>`), rejecting mismatched wire types via `None` and
//! filtering out NaN/inf through [`finite_f64`].
//!
//! Owns: [`extract_tag_f64`], [`extract_tag_u64`], [`extract_f32_vec_all`],
//!       and sibling accessors.
//! Does not own: sample extraction, vendor-specific parsing, or serialization.

use telemetry_parser::tags_impl::{TagId, TagMap, TagValue};

use crate::media::telemetry_math::finite_f64;

/// Reads numeric telemetry tags through one conversion path.
pub(crate) fn extract_tag_f64(map: &TagMap, id: &TagId) -> Option<f64> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::u8(value) => finite_f64(*value.get() as f64),
        TagValue::i8(value) => finite_f64(*value.get() as f64),
        TagValue::u16(value) => finite_f64(*value.get() as f64),
        TagValue::i16(value) => finite_f64(*value.get() as f64),
        TagValue::u32(value) => finite_f64(*value.get() as f64),
        TagValue::i32(value) => finite_f64(*value.get() as f64),
        TagValue::u64(value) => finite_f64(*value.get() as f64),
        TagValue::i64(value) => finite_f64(*value.get() as f64),
        TagValue::f32(value) => finite_f64(*value.get() as f64),
        TagValue::f64(value) => finite_f64(*value.get()),
        _ => None,
    }
}

/// Extracts a rational value from a `u32x2` (num, den) pair.
///
/// Canon stores shutter speed as `Custom("ShutterSpeed2")` with a u32x2 value
/// where the pair is `(den, num)` and the exposure time = `num / den` seconds.
pub(crate) fn extract_tag_u32x2_rational(map: &TagMap, id: &TagId) -> Option<f64> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::u32x2(value) => {
            let (a, b) = *value.get();
            if a != 0 {
                let v = b as f64 / a as f64;
                if v.is_finite() && v > 0.0 {
                    return Some(v);
                }
            }
            if b != 0 {
                let v = a as f64 / b as f64;
                if v.is_finite() && v > 0.0 {
                    return Some(v);
                }
            }
            None
        }
        _ => None,
    }
}

/// Reads integer vector companion tags such as GPMF `SCAL`.
pub(crate) fn extract_tag_i32_vec<'a>(map: &'a TagMap, id: &TagId) -> Option<&'a [i32]> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::Vec_i32(value) => Some(value.get()),
        _ => None,
    }
}

/// Reads integer timestamp fields (e.g. GoPro `GPSU`).
pub(crate) fn extract_tag_u64(map: &TagMap, id: &TagId) -> Option<u64> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::u64(value) => Some(*value.get()),
        _ => None,
    }
}

/// Reads string tags only when the parser exposed them as structured strings.
pub(crate) fn extract_tag_string(map: &TagMap, id: &TagId) -> Option<String> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::String(value) => Some(value.get().clone()),
        _ => None,
    }
}

/// Extracts all elements from a `Vec_f32` tag value.
pub(crate) fn extract_f32_vec_all(map: &TagMap, id: &TagId) -> Option<Vec<f64>> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::Vec_f32(values) => Some(values.get().iter().map(|x| *x as f64).collect()),
        _ => None,
    }
}

/// Extracts all elements from a `Vec_u16` tag value.
pub(crate) fn extract_u16_vec_all(map: &TagMap, id: &TagId) -> Option<Vec<f64>> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::Vec_u16(values) => Some(values.get().iter().map(|x| *x as f64).collect()),
        _ => None,
    }
}
