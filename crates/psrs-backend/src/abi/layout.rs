//! Canonical ABI memory layout for a WIT value.
//!
//! Computes the canonical `(size, align)` and the scalar slots of a value in
//! WIT declaration order. Shared by indirect parameter records and by non-byte
//! list elements that are themselves aggregates.

use super::WasiParamKind;

/// One scalar slot of a canonical value at a byte offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MemorySlot {
    pub offset: u32,
    pub kind: SlotKind,
}

/// The store/load width and type of a canonical slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Byte,
    Half,
    Word,
    I64,
    F32,
    F64,
}

/// The canonical size, alignment, and scalar slots of a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemoryLayout {
    pub size: u32,
    pub align: u32,
    pub slots: Vec<MemorySlot>,
}

/// The canonical layout of a directly flattenable parameter kind. `None` when
/// the kind is not directly flattenable (a nested list or an unsupported shape).
pub(crate) fn parameter_layout(kind: &WasiParamKind) -> Option<MemoryLayout> {
    match kind {
        WasiParamKind::Boolean => scalar_layout(1, SlotKind::Byte),
        WasiParamKind::Integer32 | WasiParamKind::Char | WasiParamKind::Handle(_) => {
            scalar_layout(4, SlotKind::Word)
        }
        WasiParamKind::IntegerNarrow { bits, .. } => {
            let width = u32::from(*bits) / 8;
            scalar_layout(width, slot_for_width(width))
        }
        WasiParamKind::Scalar64 { .. } => scalar_layout(8, SlotKind::I64),
        WasiParamKind::Float32 => scalar_layout(4, SlotKind::F32),
        WasiParamKind::Float64 => scalar_layout(8, SlotKind::F64),
        WasiParamKind::Enum { cases } => {
            let width = discriminant_width(cases.len());
            scalar_layout(width, slot_for_width(width))
        }
        WasiParamKind::Flags { names } => flags_layout(names.len()),
        // A byte or non-byte list is a `(pointer, length)` pair.
        WasiParamKind::List | WasiParamKind::ValueList { .. } => Some(MemoryLayout {
            size: 8,
            align: 4,
            slots: vec![
                MemorySlot {
                    offset: 0,
                    kind: SlotKind::Word,
                },
                MemorySlot {
                    offset: 4,
                    kind: SlotKind::Word,
                },
            ],
        }),
        WasiParamKind::Record { fields } => {
            record_layout(fields.iter().map(|field| parameter_layout(&field.kind)))
        }
        // A mapped aggregate is a discriminant followed by the joined payload.
        WasiParamKind::Option { payload } => aggregate_layout(&[None, Some(payload)]),
        WasiParamKind::Result { ok, err } => aggregate_layout(&[Some(ok), Some(err)]),
        WasiParamKind::Variant { cases } => {
            let cases = cases
                .iter()
                .map(|case| case.kind.as_deref())
                .collect::<Vec<_>>();
            aggregate_layout(&cases)
        }
        WasiParamKind::Unsupported => None,
    }
}

/// The canonical layout of a variant: its discriminant followed by the joined
/// case payload. Cases must agree on each payload slot's offset, and their
/// kinds must be joinable.
fn aggregate_layout(cases: &[Option<&WasiParamKind>]) -> Option<MemoryLayout> {
    let discriminant = discriminant_width(cases.len());
    let mut payload_size = 0_u32;
    let mut payload_align = 1_u32;
    let mut positions: Vec<(u32, SlotKind)> = Vec::new();
    for case in cases {
        let layout = match case {
            Some(kind) => parameter_layout(kind)?,
            None => MemoryLayout {
                size: 0,
                align: 1,
                slots: Vec::new(),
            },
        };
        payload_size = payload_size.max(layout.size);
        payload_align = payload_align.max(layout.align);
        for (index, slot) in layout.slots.iter().enumerate() {
            match positions.get_mut(index) {
                Some(existing) => {
                    if existing.0 != slot.offset {
                        return None;
                    }
                    existing.1 = join_slot_kind(existing.1, slot.kind)?;
                }
                None => positions.push((slot.offset, slot.kind)),
            }
        }
    }
    let payload_offset = align_to(discriminant, payload_align)?;
    let mut slots = vec![MemorySlot {
        offset: 0,
        kind: slot_for_width(discriminant),
    }];
    for (offset, kind) in positions {
        slots.push(MemorySlot {
            offset: payload_offset.checked_add(offset)?,
            kind,
        });
    }
    let align = discriminant.max(payload_align);
    Some(MemoryLayout {
        size: align_to(payload_offset.checked_add(payload_size)?, align)?,
        align,
        slots,
    })
}

/// The canonical slot kind of two joined case payloads. Integer widths widen to
/// `Word`; a float pair widens to `F64`; anything else is unjoinable.
fn join_slot_kind(left: SlotKind, right: SlotKind) -> Option<SlotKind> {
    if left == right {
        return Some(left);
    }
    match (left, right) {
        (
            SlotKind::Byte | SlotKind::Half | SlotKind::Word,
            SlotKind::Byte | SlotKind::Half | SlotKind::Word,
        ) => Some(SlotKind::Word),
        (SlotKind::F32, SlotKind::F64) | (SlotKind::F64, SlotKind::F32) => Some(SlotKind::F64),
        _ => None,
    }
}

/// The canonical byte offset of a variant payload, aligned to the maximum case
/// alignment. `cases` are the payload kinds in tag order. The discriminant
/// width follows the case count, matching the canonical ABI `variant`.
pub(crate) fn variant_payload_offset(cases: &[Option<&WasiParamKind>]) -> Option<u32> {
    let mut max_align = 1_u32;
    for kind in cases.iter().flatten() {
        max_align = max_align.max(case_alignment(kind)?);
    }
    align_to(discriminant_width(cases.len()), max_align)
}

/// The canonical alignment of one variant case payload. A non-byte list is a
/// `(pointer, length)` pair, so it aligns like a byte list; a nested variant
/// aligns to its discriminant and its widest case.
fn case_alignment(kind: &WasiParamKind) -> Option<u32> {
    match kind {
        WasiParamKind::List | WasiParamKind::ValueList { .. } => Some(4),
        WasiParamKind::Option { payload } => Some(case_alignment(payload)?.max(1)),
        WasiParamKind::Result { ok, err } => {
            Some(case_alignment(ok)?.max(case_alignment(err)?).max(1))
        }
        WasiParamKind::Variant { cases } => {
            let mut align = discriminant_width(cases.len());
            for case in cases {
                if let Some(kind) = &case.kind {
                    align = align.max(case_alignment(kind)?);
                }
            }
            Some(align)
        }
        other => parameter_layout(other).map(|layout| layout.align),
    }
}

/// The canonical layout of a sequence of fields, padded and aligned.
pub(crate) fn record_layout(
    fields: impl IntoIterator<Item = Option<MemoryLayout>>,
) -> Option<MemoryLayout> {
    let mut size = 0_u32;
    let mut align = 1_u32;
    let mut slots = Vec::new();
    for field in fields {
        let field = field?;
        size = align_to(size, field.align)?;
        for slot in field.slots {
            slots.push(MemorySlot {
                offset: size.checked_add(slot.offset)?,
                kind: slot.kind,
            });
        }
        size = size.checked_add(field.size)?;
        align = align.max(field.align);
    }
    size = align_to(size, align)?;
    Some(MemoryLayout { size, align, slots })
}

/// The canonical byte offset and layout of each record field, in declaration
/// order. `None` when a field has no directly flattenable layout.
pub(crate) fn record_fields(
    fields: impl IntoIterator<Item = Option<MemoryLayout>>,
) -> Option<Vec<(u32, MemoryLayout)>> {
    let mut offset = 0_u32;
    let mut layouts = Vec::new();
    for field in fields {
        let field = field?;
        offset = align_to(offset, field.align)?;
        layouts.push((offset, field.clone()));
        offset = offset.checked_add(field.size)?;
    }
    Some(layouts)
}

fn scalar_layout(size: u32, kind: SlotKind) -> Option<MemoryLayout> {
    Some(MemoryLayout {
        size,
        align: size,
        slots: vec![MemorySlot { offset: 0, kind }],
    })
}

fn flags_layout(count: usize) -> Option<MemoryLayout> {
    let (width, words) = match count {
        0 => {
            return Some(MemoryLayout {
                size: 0,
                align: 4,
                slots: Vec::new(),
            });
        }
        1..=8 => (1, 1),
        9..=16 => (2, 1),
        17..=32 => (4, 1),
        _ => (4, count.div_ceil(32)),
    };
    let kind = slot_for_width(width);
    let mut slots = Vec::with_capacity(words);
    for index in 0..words {
        let offset = u32::try_from(index).ok()?.checked_mul(width)?;
        slots.push(MemorySlot { offset, kind });
    }
    let size = u32::try_from(words).ok()?.checked_mul(width)?;
    Some(MemoryLayout {
        size,
        align: width,
        slots,
    })
}

/// The canonical discriminant width in bytes for a variant with `cases` cases.
pub(crate) fn discriminant_width(cases: usize) -> u32 {
    const U8_MAX: usize = u8::MAX as usize;
    const U16_MAX: usize = u16::MAX as usize;
    match cases.saturating_sub(1) {
        0..=U8_MAX => 1,
        256..=U16_MAX => 2,
        _ => 4,
    }
}

fn slot_for_width(width: u32) -> SlotKind {
    match width {
        1 => SlotKind::Byte,
        2 => SlotKind::Half,
        4 => SlotKind::Word,
        _ => unreachable!("canonical ABI discriminants use 1, 2, or 4 bytes"),
    }
}

fn align_to(value: u32, align: u32) -> Option<u32> {
    value
        .checked_add(align.checked_sub(1)?)
        .map(|value| value & !(align - 1))
}
