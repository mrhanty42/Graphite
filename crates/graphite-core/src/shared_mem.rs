use graphite_api::protocol::*;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct SharedRegion {
    ptr: *mut u8,
    len: usize,
}

unsafe impl Send for SharedRegion {}
unsafe impl Sync for SharedRegion {}

impl SharedRegion {
    pub unsafe fn try_from_raw(ptr: *mut u8, len: usize) -> Result<Self, &'static str> {
        if ptr.is_null() {
            return Err("SharedRegion: null pointer");
        }
        if len == 0 {
            return Err("SharedRegion: buffer too small");
        }
        if (ptr as usize) % 8 != 0 {
            return Err("SharedRegion: pointer must be 8-byte aligned");
        }
        Ok(Self { ptr, len })
    }

    #[inline]
    pub fn snapshot_ready(&self) -> bool {
        assert!((OFFSET_SNAPSHOT_READY + 4) <= self.len);
        unsafe { (*(self.ptr.add(OFFSET_SNAPSHOT_READY) as *const AtomicU32)).load(Ordering::Acquire) != 0 }
    }

    #[inline]
    pub fn clear_snapshot_ready(&self) {
        assert!((OFFSET_SNAPSHOT_READY + 4) <= self.len);
        unsafe { (*(self.ptr.add(OFFSET_SNAPSHOT_READY) as *const AtomicU32)).store(0, Ordering::Release) }
    }

    #[inline]
    pub fn set_command_count(&self, count: u32) {
        assert!((OFFSET_COMMAND_COUNT + 4) <= self.len);
        unsafe {
            (*(self.ptr.add(OFFSET_COMMAND_COUNT) as *const AtomicU32)).store(count, Ordering::Release)
        }
    }

    pub fn world_snapshot_ptr(&self) -> *const u8 {
        assert!(OFFSET_WORLD_SNAPSHOT < self.len);
        unsafe { self.ptr.add(OFFSET_WORLD_SNAPSHOT) as *const u8 }
    }

    pub fn command_queue_ptr(&self) -> *mut u8 {
        assert!(OFFSET_COMMAND_QUEUE < self.len);
        unsafe { self.ptr.add(OFFSET_COMMAND_QUEUE) }
    }

    pub fn command_queue_ptr_mut(&self) -> *mut u8 {
        self.command_queue_ptr()
    }

    #[inline]
    pub fn command_queue_tail(&self) -> u32 {
        assert!((OFFSET_COMMAND_QUEUE + CQ_TAIL_OFFSET + 4) <= self.len);
        unsafe {
            (&*self.ptr
                .add(OFFSET_COMMAND_QUEUE + CQ_TAIL_OFFSET)
                .cast::<AtomicU32>())
                .load(Ordering::Acquire)
        }
    }
}
