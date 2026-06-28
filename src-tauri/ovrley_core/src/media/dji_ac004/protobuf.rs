//! Minimal protobuf wire-format decoder for the DJI AC004 metadata subset.
//!
//! DJI AC004 stores GPS samples as protobuf-encoded messages in a dedicated
//! `DJI meta` MP4 track. Rather than requiring `protoc` at build time or
//! pulling in a heavyweight protobuf runtime, this module implements the
//! subset of the wire format that the AC004 schema uses — varint, fixed64,
//! fixed32, and length-delimited fields. Unknown wire types return `None` so
//! malformed or schema-mismatched data fails closed instead of risking
//! desynchronisation.
//!
//! Owns: [`decode_field`], [`decode_varint`], [`FieldIter`], [`get_submessage`],
//!       [`get_varint`], [`get_f64`], [`get_f32`], [`get_string`].
//! Does not own: AC004 GPS-fix parsing (see [`super::parser`]), inspection
//!       (see [`super::inspect`]), or FFmpeg integration (see [`super`]).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WireValue<'a> {
    Varint(u64),
    Fixed64(&'a [u8]),
    LengthDelimited(&'a [u8]),
    Fixed32(&'a [u8]),
}

/// Decodes one protobuf field from a bounded byte slice.
///
/// The fallback only implements wire types used by the observed AC004 metadata:
/// varint, fixed64, length-delimited, and fixed32. Unsupported wire types return
/// `None` so higher-level parsing stops instead of trying to resynchronize from
/// an ambiguous byte position.
pub fn decode_field(data: &[u8], pos: usize) -> Option<(u64, WireValue<'_>, usize)> {
    if pos >= data.len() {
        return None;
    }

    let (tag, mut pos) = decode_varint(data, pos)?;
    let field_num = tag >> 3;
    let wire_type = tag & 0x07;

    match wire_type {
        0 => {
            let (value, next_pos) = decode_varint(data, pos)?;
            Some((field_num, WireValue::Varint(value), next_pos))
        }
        1 => {
            let end = pos.checked_add(8)?;
            if end <= data.len() {
                Some((field_num, WireValue::Fixed64(&data[pos..end]), end))
            } else {
                None
            }
        }
        2 => {
            let (length, next_pos) = decode_varint(data, pos)?;
            pos = next_pos;
            let end = pos.checked_add(length as usize)?;
            if end <= data.len() {
                Some((field_num, WireValue::LengthDelimited(&data[pos..end]), end))
            } else {
                None
            }
        }
        5 => {
            let end = pos.checked_add(4)?;
            if end <= data.len() {
                Some((field_num, WireValue::Fixed32(&data[pos..end]), end))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Decodes a protobuf base-128 varint and returns the next unread byte offset.
///
/// The ten-byte limit mirrors protobuf's 64-bit integer bound. Treating longer
/// runs as malformed prevents an unterminated varint from scanning arbitrary
/// data while looking for a continuation bit that never arrives.
pub fn decode_varint(data: &[u8], mut pos: usize) -> Option<(u64, usize)> {
    let mut result = 0u64;
    let mut shift = 0u32;

    while pos < data.len() && shift < 70 {
        let byte = data[pos];
        result |= u64::from(byte & 0x7f) << shift;
        pos += 1;
        if byte & 0x80 == 0 {
            return Some((result, pos));
        }
        shift += 7;
    }

    None
}

/// Creates a forward-only protobuf field iterator over a message slice.
pub fn iter_fields(data: &[u8]) -> FieldIter<'_> {
    FieldIter { data, pos: 0 }
}

pub struct FieldIter<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for FieldIter<'a> {
    type Item = (u64, WireValue<'a>);

    /// Advances by exactly one protobuf field and stops on malformed input.
    fn next(&mut self) -> Option<Self::Item> {
        let (field_num, value, next_pos) = decode_field(self.data, self.pos)?;
        self.pos = next_pos;
        Some((field_num, value))
    }
}

/// Returns the first nested message stored in the requested protobuf field.
pub fn get_submessage(data: &[u8], target_field: u64) -> Option<&[u8]> {
    iter_fields(data).find_map(|(field_num, value)| {
        if field_num == target_field {
            if let WireValue::LengthDelimited(bytes) = value {
                return Some(bytes);
            }
        }
        None
    })
}

/// Returns the first unsigned integer field matching the requested number.
pub fn get_varint(data: &[u8], target_field: u64) -> Option<u64> {
    iter_fields(data).find_map(|(field_num, value)| {
        if field_num == target_field {
            if let WireValue::Varint(value) = value {
                return Some(value);
            }
        }
        None
    })
}

/// Returns the first little-endian double field matching the requested number.
pub fn get_f64(data: &[u8], target_field: u64) -> Option<f64> {
    iter_fields(data).find_map(|(field_num, value)| {
        if field_num == target_field {
            if let WireValue::Fixed64(bytes) = value {
                return bytes.try_into().ok().map(f64::from_le_bytes);
            }
        }
        None
    })
}

/// Returns the first little-endian float field matching the requested number.
pub fn get_f32(data: &[u8], target_field: u64) -> Option<f32> {
    iter_fields(data).find_map(|(field_num, value)| {
        if field_num == target_field {
            if let WireValue::Fixed32(bytes) = value {
                return bytes.try_into().ok().map(f32::from_le_bytes);
            }
        }
        None
    })
}

/// Decodes a length-delimited field as UTF-8 when the schema expects text.
pub fn get_string(data: &[u8], target_field: u64) -> Option<String> {
    let bytes = get_submessage(data, target_field)?;
    std::str::from_utf8(bytes).ok().map(str::to_string)
}
