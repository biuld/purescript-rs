//! Checked acquisition of fresh segments up to `memory.size`, then `memory.grow`.
//!
//! The cursor advances only after a segment is published. A failed grow leaves
//! the cursor where it was. Returned pages are never shrunk.

pub const PAGE: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub start: usize,
    pub bytes: usize,
    pub end: usize,
    pub pages: usize,
}

/// Plans a page-aligned segment at or above `cursor`.
///
/// `pages` is zero when the current memory already covers the segment.
pub fn plan(cursor: usize, request: usize, mem_size: usize) -> Result<Segment, ()> {
    if request == 0 || !PAGE.is_power_of_two() {
        return Err(());
    }
    let start = align_up(cursor, PAGE)?;
    let bytes = align_up(request, PAGE)?;
    let end = start.checked_add(bytes).ok_or(())?;
    let pages = if end > mem_size {
        end.checked_sub(mem_size).ok_or(())?.div_ceil(PAGE)
    } else {
        0
    };
    Ok(Segment {
        start,
        bytes,
        end,
        pages,
    })
}

pub fn align_up(value: usize, align: usize) -> Result<usize, ()> {
    if align == 0 || !align.is_power_of_two() {
        return Err(());
    }
    let padded = value.checked_add(align - 1).ok_or(())?;
    Ok(padded & !(align - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_in_range_segment_does_not_grow_or_move_the_failed_cursor() {
        let segment = plan(196_608, 100, 262_144).unwrap();
        assert_eq!(segment.pages, 0);
        assert_eq!(segment.start, 196_608);
        assert_eq!(segment.bytes, PAGE);
        assert!(plan(196_608, PAGE, 196_608).unwrap().pages >= 1);
        assert!(plan(usize::MAX - 8, PAGE, usize::MAX).is_err());
    }
}
