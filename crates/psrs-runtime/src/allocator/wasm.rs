//! Canonical realloc adapter over one `dlmalloc` 0.2.14 instance.
//!
//! The instance is created on the first call, after the heap-boundary import
//! can be read. No start function allocates. Reentry while the instance is
//! borrowed traps.

use super::{algorithm, phase, provision};

use core::cell::Cell;
use dlmalloc::{Allocator, Dlmalloc};

const PAGE: usize = provision::PAGE;

struct Provision {
    cursor: Cell<usize>,
}

unsafe impl Send for Provision {}

unsafe impl Allocator for Provision {
    fn alloc(&self, size: usize) -> (*mut u8, usize, u32) {
        let cursor = self.cursor.get();
        let Ok(segment) = provision::plan(cursor, size, memory_bytes()) else {
            return (core::ptr::null_mut(), 0, 0);
        };
        if segment.pages > 0 {
            let grown = core::arch::wasm32::memory_grow(0, segment.pages);
            if grown == usize::MAX || segment.end > memory_bytes() {
                return (core::ptr::null_mut(), 0, 0);
            }
        }
        self.cursor.set(segment.end);
        (segment.start as *mut u8, segment.bytes, 0)
    }

    fn remap(&self, _: *mut u8, _: usize, _: usize, _: bool) -> *mut u8 {
        core::ptr::null_mut()
    }

    fn free_part(&self, _: *mut u8, _: usize, _: usize) -> bool {
        false
    }

    fn free(&self, _: *mut u8, _: usize) -> bool {
        false
    }

    fn can_release_part(&self, _: u32) -> bool {
        false
    }

    fn allocates_zeros(&self) -> bool {
        true
    }

    fn page_size(&self) -> usize {
        PAGE
    }
}

struct WasmStore<'a> {
    alloc: &'a mut Dlmalloc<Provision>,
}

impl algorithm::Store for WasmStore<'_> {
    fn allocate(&mut self, size: u32, align: u32) -> Option<u32> {
        let pointer = unsafe { self.alloc.malloc(size as usize, align as usize) };
        if pointer.is_null() {
            None
        } else {
            Some(pointer as u32)
        }
    }

    fn release(&mut self, pointer: u32, size: u32, align: u32) {
        unsafe {
            self.alloc
                .free(pointer as *mut u8, size as usize, align as usize);
        }
    }

    fn read(&self, address: u32) -> u32 {
        unsafe { core::ptr::read_unaligned(address as *const u32) }
    }

    fn write(&mut self, address: u32, value: u32) {
        unsafe { core::ptr::write_unaligned(address as *mut u32, value) }
    }

    fn copy(&mut self, from: u32, to: u32, len: u32) {
        if len == 0 || from == to {
            return;
        }
        unsafe {
            core::ptr::copy(from as *const u8, to as *mut u8, len as usize);
        }
    }
}

// The application supplies the final planned boundary through a real Wasm import.
#[link(wasm_import_module = "__main_module__")]
unsafe extern "C" {
    fn get_heap_base() -> u32;
}

// Keep one initialized storage object: its active data segment initializes
// every field without an executable BSS-zeroing start function.
struct State {
    phase: u8,
    head: u32,
    alloc: Dlmalloc<Provision>,
}

static mut STATE: State = State {
    phase: phase::UNINITIALIZED,
    head: 0,
    alloc: Dlmalloc::new_with_allocator(Provision {
        cursor: Cell::new(0),
    }),
};

fn memory_bytes() -> usize {
    core::arch::wasm32::memory_size(0) * PAGE
}

fn initialize(alloc: &mut Dlmalloc<Provision>) -> Result<(), ()> {
    let heap = unsafe { get_heap_base() } as usize;
    if heap == 0 || !heap.is_multiple_of(8) || heap > memory_bytes() {
        return Err(());
    }
    alloc.allocator_mut().cursor.set(heap);
    Ok(())
}

/// Canonical realloc export. A trap leaves no published replacement pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cabi_realloc(old_ptr: i32, old_len: i32, align: i32, new_len: i32) -> i32 {
    unsafe {
        let state = &mut *core::ptr::addr_of_mut!(STATE);
        let initializing = phase::enter(&mut state.phase).unwrap_or_else(|_| {
            core::arch::wasm32::unreachable();
        });
        if initializing {
            if initialize(&mut state.alloc).is_err() {
                core::arch::wasm32::unreachable();
            }
        }
        let mut store = WasmStore {
            alloc: &mut state.alloc,
        };
        let result = algorithm::realloc(
            &mut store,
            &mut state.head,
            old_ptr,
            old_len,
            align,
            new_len,
        );
        phase::ready(&mut state.phase);
        result.unwrap_or_else(|_| core::arch::wasm32::unreachable())
    }
}
