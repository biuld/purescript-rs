//! Canonical `cabi_realloc` steps over a caller-supplied backing store.
//!
//! Live records sit in a prefix of each backing allocation. Lookup walks those
//! trusted records. It does not form a header address from the caller's pointer.

const RECORD: u32 = 20;

pub trait Store {
    fn allocate(&mut self, size: u32, align: u32) -> Option<u32>;
    fn release(&mut self, pointer: u32, size: u32, align: u32);
    fn read(&self, address: u32) -> u32;
    fn write(&mut self, address: u32, value: u32);
    fn copy(&mut self, from: u32, to: u32, len: u32);
}

#[derive(Clone, Copy)]
struct Live {
    backing: u32,
    next: u32,
    user: u32,
    requested: u32,
    size: u32,
    align: u32,
}

/// Runs one canonical call. `Err` is a trap. `Ok` is the returned pointer.
pub fn realloc(
    store: &mut impl Store,
    head: &mut u32,
    old_ptr: i32,
    old_len: i32,
    align: i32,
    new_len: i32,
) -> Result<i32, ()> {
    let align = canonical_align(align)?;
    let new_len = canonical_len(new_len)?;
    if new_len == 0 {
        if old_ptr != 0 {
            let live = find(store, *head, old_ptr)?;
            if live.requested != canonical_len(old_len)? {
                return Err(());
            }
            unlink(store, head, live.backing);
            store.release(live.backing, live.size, live.align);
        }
        return Ok(0);
    }
    if old_ptr == 0 {
        if old_len != 0 {
            return Err(());
        }
        return Ok(insert(store, head, align, new_len)? as i32);
    }
    let old_len = canonical_len(old_len)?;
    let live = find(store, *head, old_ptr)?;
    if live.requested != old_len {
        return Err(());
    }
    let user = insert(store, head, align, new_len)?;
    store.copy(live.user, user, old_len.min(new_len));
    unlink(store, head, live.backing);
    store.release(live.backing, live.size, live.align);
    Ok(user as i32)
}

fn insert(store: &mut impl Store, head: &mut u32, align: u32, len: u32) -> Result<u32, ()> {
    let backing_align = align.max(4);
    let user_offset = provision::align_u32(RECORD, align)?;
    let size = user_offset.checked_add(len).ok_or(())?;
    let backing = store.allocate(size, backing_align).ok_or(())?;
    let user = backing.checked_add(user_offset).ok_or(())?;
    if user == 0 {
        store.release(backing, size, backing_align);
        return Err(());
    }
    store.write(backing, *head);
    store.write(backing + 4, user);
    store.write(backing + 8, len);
    store.write(backing + 12, size);
    store.write(backing + 16, backing_align);
    *head = backing;
    Ok(user)
}

fn find(store: &impl Store, head: u32, old_ptr: i32) -> Result<Live, ()> {
    if old_ptr <= 0 {
        return Err(());
    }
    let wanted = old_ptr as u32;
    let mut backing = head;
    while backing != 0 {
        let live = read_live(store, backing)?;
        if live.user == wanted {
            return Ok(live);
        }
        backing = live.next;
    }
    Err(())
}

fn read_live(store: &impl Store, backing: u32) -> Result<Live, ()> {
    Ok(Live {
        backing,
        next: store.read(backing),
        user: store.read(backing + 4),
        requested: store.read(backing + 8),
        size: store.read(backing + 12),
        align: store.read(backing + 16),
    })
}

fn unlink(store: &mut impl Store, head: &mut u32, backing: u32) {
    if *head == backing {
        *head = store.read(backing);
        return;
    }
    let mut previous = *head;
    while previous != 0 {
        let next = store.read(previous);
        if next == backing {
            store.write(previous, store.read(backing));
            return;
        }
        previous = next;
    }
}

fn canonical_align(align: i32) -> Result<u32, ()> {
    if align <= 0 {
        return Err(());
    }
    let align = align as u32;
    if align.is_power_of_two() {
        Ok(align)
    } else {
        Err(())
    }
}

fn canonical_len(len: i32) -> Result<u32, ()> {
    u32::try_from(len).map_err(|_| ())
}

pub mod provision {
    pub fn align_u32(value: u32, align: u32) -> Result<u32, ()> {
        if align == 0 || !align.is_power_of_two() {
            return Err(());
        }
        let padded = value.checked_add(align - 1).ok_or(())?;
        Ok(padded & !(align - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Heap {
        bytes: Vec<u8>,
        cursor: u32,
        fail_after: u32,
        allocs: u32,
    }

    impl Heap {
        fn new() -> Self {
            Self {
                bytes: vec![0; 64],
                cursor: 64,
                fail_after: u32::MAX,
                allocs: 0,
            }
        }
    }

    impl Store for Heap {
        fn allocate(&mut self, size: u32, align: u32) -> Option<u32> {
            self.allocs += 1;
            if self.allocs > self.fail_after {
                return None;
            }
            let start = provision::align_u32(self.cursor, align).ok()?;
            let end = start.checked_add(size)?;
            self.bytes.resize(end as usize, 0);
            self.cursor = end;
            Some(start)
        }

        fn release(&mut self, _: u32, _: u32, _: u32) {}

        fn read(&self, address: u32) -> u32 {
            let offset = address as usize;
            u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
        }

        fn write(&mut self, address: u32, value: u32) {
            let offset = address as usize;
            self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }

        fn copy(&mut self, from: u32, to: u32, len: u32) {
            for index in 0..len {
                let byte = self.bytes[(from + index) as usize];
                self.bytes[(to + index) as usize] = byte;
            }
        }
    }

    #[test]
    fn alignment_resize_copy_and_rejection_share_one_registry() {
        let mut heap = Heap::new();
        let mut head = 0;
        let first = realloc(&mut heap, &mut head, 0, 0, 8, 4).unwrap();
        heap.write(first as u32, u32::from_le_bytes(*b"abcd"));
        assert_eq!(first & 7, 0);
        let resized = realloc(&mut heap, &mut head, first, 4, 32, 4).unwrap();
        assert_eq!(resized & 31, 0);
        assert_eq!(&heap.bytes[resized as usize..resized as usize + 4], b"abcd");
        assert!(realloc(&mut heap, &mut head, 0, 0, 3, 4).is_err());
        assert!(realloc(&mut heap, &mut head, 0, 1, 8, 4).is_err());
        assert!(realloc(&mut heap, &mut head, 32, 4, 8, 4).is_err());
        assert!(realloc(&mut heap, &mut head, resized, 3, 8, 4).is_err());
        assert_eq!(realloc(&mut heap, &mut head, resized, 4, 8, 0).unwrap(), 0);
        let again = realloc(&mut heap, &mut head, 0, 0, 8, 4).unwrap();
        assert_ne!(again, 0);
    }

    #[test]
    fn a_failed_replacement_keeps_the_old_allocation() {
        let mut heap = Heap::new();
        heap.fail_after = 1;
        let mut head = 0;
        let first = realloc(&mut heap, &mut head, 0, 0, 8, 4).unwrap();
        heap.bytes[first as usize] = 9;
        assert!(realloc(&mut heap, &mut head, first, 4, 8, 8).is_err());
        assert_eq!(heap.bytes[first as usize], 9);
        assert_eq!(
            head,
            first as u32 - provision::align_u32(RECORD, 8).unwrap()
        );
    }
}
