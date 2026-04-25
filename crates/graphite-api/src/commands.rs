use crate::protocol::*;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct CommandQueue {
    base: *mut u8,
    capacity: usize,
}

unsafe impl Send for CommandQueue {}

impl CommandQueue {
    pub fn reset(&self) {
        self.head_atomic().store(0, Ordering::Release);
        self.tail_atomic().store(0, Ordering::Release);
    }

    /// Creates a CommandQueue reference from a raw pointer without initialization.
    ///
    /// This method assumes that the queue has already been initialized via `from_raw()`.
    /// It does not reinitialize head, tail, or capacity fields.
    ///
    /// # Safety
    ///
    /// - `ptr` must point to a valid CommandQueue region previously initialized with `from_raw()`
    /// - `ptr` must be properly aligned (8-byte)
    /// - The queue must have been initialized before calling this method
    pub unsafe fn from_raw_ptr(ptr: *mut u8) -> Self {
        Self {
            base: ptr,
            capacity: CQ_CAPACITY,
        }
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, state_id: u32) -> bool {
        let mut payload = [0_u8; 16];
        payload[0..4].copy_from_slice(&x.to_le_bytes());
        payload[4..8].copy_from_slice(&y.to_le_bytes());
        payload[8..12].copy_from_slice(&z.to_le_bytes());
        payload[12..16].copy_from_slice(&state_id.to_le_bytes());
        self.push(CMD_SET_BLOCK, &payload)
    }

    pub fn send_chat(&mut self, player_id: u32, message: &str) -> bool {
        let msg_bytes = message.as_bytes();
        let msg_len = msg_bytes.len().min(249);
        let mut payload = vec![0_u8; 5 + msg_len];
        payload[0..4].copy_from_slice(&player_id.to_le_bytes());
        payload[4] = msg_len as u8;
        payload[5..].copy_from_slice(&msg_bytes[..msg_len]);
        self.push(CMD_SEND_CHAT, &payload)
    }

    pub fn spawn_particle(
        &mut self,
        particle_id: u32,
        x: f64,
        y: f64,
        z: f64,
        count: f32,
    ) -> bool {
        let mut payload = [0_u8; 32];
        payload[0..4].copy_from_slice(&particle_id.to_le_bytes());
        payload[4..12].copy_from_slice(&x.to_le_bytes());
        payload[12..20].copy_from_slice(&y.to_le_bytes());
        payload[20..28].copy_from_slice(&z.to_le_bytes());
        payload[28..32].copy_from_slice(&count.to_le_bytes());
        self.push(CMD_SPAWN_PARTICLE, &payload)
    }

    pub fn set_velocity(&mut self, entity_id: i32, vx: f32, vy: f32, vz: f32) -> bool {
        let mut payload = [0_u8; 16];
        payload[0..4].copy_from_slice(&entity_id.to_le_bytes());
        payload[4..8].copy_from_slice(&vx.to_le_bytes());
        payload[8..12].copy_from_slice(&vy.to_le_bytes());
        payload[12..16].copy_from_slice(&vz.to_le_bytes());
        self.push(CMD_SET_VELOCITY, &payload)
    }

    pub fn kill_entity(&mut self, entity_id: i32) -> bool {
        self.push(CMD_KILL_ENTITY, &entity_id.to_le_bytes())
    }

    fn push(&mut self, cmd_type: u8, payload: &[u8]) -> bool {
        if payload.len() > 254 {
            return false;
        }

        let total = 2_u32.wrapping_add(payload.len() as u32);
        let tail_u32 = self.tail_atomic().load(Ordering::Relaxed);
        let head_u32 = self.head_atomic().load(Ordering::Acquire);
        // Use wrapping_sub to handle u32 overflow correctly.
        // head and tail are monotonically increasing and wrap around u32.
        // When tail overflows before head, tail < head, and wrapping_sub correctly
        // computes the actual used space in the circular buffer.
        // saturating_sub would incorrectly return 0, allowing new data to overwrite
        // unprocessed commands.
        let used = tail_u32.wrapping_sub(head_u32) as usize;

        let tail_usize = tail_u32 as usize;
        if used + total as usize >= self.capacity {
            log::warn!(
                "[Graphite] CommandQueue full, dropping command {:02x}",
                cmd_type
            );
            return false;
        }

        let data_base = unsafe { self.base.add(CQ_DATA_OFFSET) };
        unsafe {
            *data_base.add(tail_usize % self.capacity) = cmd_type;
            *data_base.add((tail_usize.wrapping_add(1)) % self.capacity) = payload.len() as u8;
        }
        for (index, byte) in payload.iter().copied().enumerate() {
            unsafe {
                *data_base.add((tail_usize.wrapping_add(2 + index)) % self.capacity) = byte;
            }
        }

        self.tail_atomic()
            .store(tail_u32.wrapping_add(total), Ordering::Release);
        true
    }

    fn head_atomic(&self) -> &AtomicU32 {
        unsafe { &*self.base.add(CQ_HEAD_OFFSET).cast::<AtomicU32>() }
    }

    fn tail_atomic(&self) -> &AtomicU32 {
        unsafe { &*self.base.add(CQ_TAIL_OFFSET).cast::<AtomicU32>() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alloc_queue_mem() -> Vec<u8> {
        let mut mem = vec![0_u8; CQ_DATA_OFFSET + CQ_CAPACITY + 64];
        let base = mem.as_mut_ptr();
        unsafe {
            base.add(CQ_HEAD_OFFSET).cast::<u32>().write(0);
            base.add(CQ_TAIL_OFFSET).cast::<u32>().write(0);
            base.add(CQ_CAPACITY_OFFSET)
                .cast::<u32>()
                .write(CQ_CAPACITY as u32);
        }
        mem
    }

    #[test]
    fn push_rejects_oversized_payload_without_panicking() {
        let mut mem = alloc_queue_mem();
        let mut q = unsafe { CommandQueue::from_raw_ptr(mem.as_mut_ptr()) };

        let payload = vec![0_u8; 255];
        assert!(!q.push(0xAA, &payload));
        assert_eq!(q.tail_atomic().load(Ordering::Relaxed), 0);
    }

    #[test]
    fn push_writes_header_and_advances_tail() {
        let mut mem = alloc_queue_mem();
        let mut q = unsafe { CommandQueue::from_raw_ptr(mem.as_mut_ptr()) };

        let payload = [1_u8, 2, 3, 4];
        assert!(q.push(0x10, &payload));

        let data_base = CQ_DATA_OFFSET;
        assert_eq!(mem[data_base], 0x10);
        assert_eq!(mem[data_base + 1], payload.len() as u8);
        assert_eq!(&mem[data_base + 2..data_base + 6], &payload);
        assert_eq!(
            q.tail_atomic().load(Ordering::Acquire),
            (2 + payload.len()) as u32
        );
    }

    #[test]
    fn wraparound_payload_does_not_overflow() {
        let mut mem = alloc_queue_mem();
        let mut q = unsafe { CommandQueue::from_raw_ptr(mem.as_mut_ptr()) };

        // Set head close to tail so there is enough free space for the payload.
        // tail = CQ_CAPACITY - 1, head = CQ_CAPACITY - 10 → used = 9, free = plenty.
        q.head_atomic()
            .store((CQ_CAPACITY - 10) as u32, Ordering::Release);
        q.tail_atomic()
            .store((CQ_CAPACITY - 1) as u32, Ordering::Release);

        let payload = [9_u8; 3];
        assert!(q.push(0x22, &payload));

        let data_base = CQ_DATA_OFFSET;
        assert_eq!(mem[data_base + (CQ_CAPACITY - 1)], 0x22);
        assert_eq!(mem[data_base + 0], payload.len() as u8);
        assert_eq!(mem[data_base + 1], 9);
        assert_eq!(mem[data_base + 2], 9);
        assert_eq!(mem[data_base + 3], 9);
    }
}
