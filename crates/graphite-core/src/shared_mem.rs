use graphite_api::protocol::*;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct SharedRegion {
    ptr: *mut u8,
    len: usize,
}

unsafe impl Send for SharedRegion {}
unsafe impl Sync for SharedRegion {}

impl SharedRegion {
    pub unsafe fn from_raw(ptr: *mut u8, len: usize) -> Self {
        assert!(!ptr.is_null(), "SharedRegion: null pointer");
        assert!(len > 0, "SharedRegion: buffer too small");
        assert!(ptr.align_offset(8) == 0, "SharedRegion: pointer must be 8-byte aligned");
        Self { ptr, len }
    }

    #[inline]
    pub fn snapshot_ready(&self) -> bool {
        unsafe { (*(self.ptr.add(OFFSET_SNAPSHOT_READY) as *const AtomicU32)).load(Ordering::Acquire) != 0 }
    }

    #[inline]
    pub fn clear_snapshot_ready(&self) {
        unsafe { (*(self.ptr.add(OFFSET_SNAPSHOT_READY) as *const AtomicU32)).store(0, Ordering::Release) }
    }

    #[inline]
    pub fn set_command_count(&self, count: u32) {
        unsafe {
            (*(self.ptr.add(OFFSET_COMMAND_COUNT) as *const AtomicU32)).store(count, Ordering::Release)
        }
    }

    pub fn world_snapshot_ptr(&self) -> *const u8 {
        debug_assert!(OFFSET_WORLD_SNAPSHOT < self.len);
        unsafe { self.ptr.add(OFFSET_WORLD_SNAPSHOT) as *const u8 }
    }

    pub fn command_queue_ptr(&self) -> *mut u8 {
        debug_assert!(OFFSET_COMMAND_QUEUE < self.len);
        unsafe { self.ptr.add(OFFSET_COMMAND_QUEUE) }
    }

    pub fn command_queue_ptr_mut(&self) -> *mut u8 {
        self.command_queue_ptr()
    }
}
