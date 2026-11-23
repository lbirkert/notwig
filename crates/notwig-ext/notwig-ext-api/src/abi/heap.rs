use std::sync::{Mutex, OnceLock};

use crate::pack_ptr_len;

const HEAP_WORDS: usize = 128 * 1024;
const ALIGNMENT: usize = 8;

struct Heap {
    words: Box<[u64]>,
    offset: usize,
    last_start: Option<usize>,
}

static HEAP: OnceLock<Mutex<Heap>> = OnceLock::new();

fn heap() -> &'static Mutex<Heap> {
    HEAP.get_or_init(|| {
        Mutex::new(Heap {
            words: vec![0_u64; HEAP_WORDS].into_boxed_slice(),
            offset: 0,
            last_start: None,
        })
    })
}

/// Allocates `len` bytes from the fixed extension heap.
///
/// This is a bump allocator over a 1 MiB static word buffer with 8-byte
/// alignment. It never frees individual allocations and never moves, so returned
/// guest pointers stay valid for the lifetime of the WASM instance. Returns `-1`
/// for negative or overflowing lengths, or when the request does not fit.
///
/// # ABI
///
/// The generated WASM ABI exposes this as `notwig_alloc` so the host can stage
/// inputs and audio in guest linear memory.
pub fn alloc(len: i32) -> i32 {
    let Ok(len) = usize::try_from(len) else {
        return -1;
    };
    let mut heap = heap()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let offset = (heap.offset + ALIGNMENT - 1) & !(ALIGNMENT - 1);
    let Some(end) = offset.checked_add(len) else {
        return -1;
    };
    let capacity = heap.words.len() * std::mem::size_of::<u64>();
    if end > capacity {
        return -1;
    }
    let address = heap.words.as_ptr() as usize + offset;
    heap.offset = end;
    heap.last_start = Some(offset);
    i32::try_from(address).unwrap_or(-1)
}

/// Releases the most recent allocation when `ptr` matches it.
///
/// The host uses the heap as a LIFO scratch area for inputs. Calling with any
/// other pointer is a no-op, and `len` is ignored. Audio buffers and static JSON
/// outputs are intentionally not reclaimed, so this must not be used to free
/// them.
pub fn free(ptr: i32, _len: i32) {
    let mut heap = heap()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(start) = heap.last_start else {
        return;
    };
    let address = heap.words.as_ptr() as usize + start;
    if i32::try_from(address).is_ok_and(|address| address == ptr) {
        heap.offset = start;
        heap.last_start = None;
    }
}

/// Packs a stable pointer and length for an extension-owned JSON buffer.
///
/// The pointer must fit in 32 bits and the length in 32 bits, matching the WASM
/// address space. Returns `0` when either does not fit. The buffer must outlive
/// the host's read, which for spec and panel frames means until the next call on
/// the same handle.
pub fn packed_pointer(bytes: &[u8]) -> i64 {
    let Ok(ptr) = i32::try_from(bytes.as_ptr() as usize) else {
        return 0;
    };
    let Ok(len) = u32::try_from(bytes.len()) else {
        return 0;
    };
    i64::try_from(pack_ptr_len(ptr as u32, len)).unwrap_or(0)
}
