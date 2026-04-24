use crate::protocol::*;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct CommandQueue {
    base: *mut u8,
    capacity: usize,
}

unsafe impl Send for CommandQueue {}

impl CommandQueue {
    pub unsafe fn from_raw(base: *mut u8) -> Self {
        unsafe {
            base.add(CQ_HEAD_OFFSET).cast::<u32>().write(0);
            base.add(CQ_TAIL_OFFSET).cast::<u32>().write(0);
            base.add(CQ_CAPACITY_OFFSET)
                .cast::<u32>()
                .write(CQ_CAPACITY as u32);
        }
        Self {
            base,
            capacity: CQ_CAPACITY,
        }
    }

    pub fn reset(&self) {
        self.head_atomic().store(0, Ordering::Release);
        self.tail_atomic().store(0, Ordering::Release);
    }

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
        assert!(payload.len() <= 254, "payload too large: {}", payload.len());

        let total = 2 + payload.len();
        let tail = self.tail_atomic().load(Ordering::Relaxed) as usize;
        let head = self.head_atomic().load(Ordering::Acquire) as usize;
        let used = tail.saturating_sub(head);

        if used + total >= self.capacity {
            log::warn!(
                "[Graphite] CommandQueue full, dropping command {:02x}",
                cmd_type
            );
            return false;
        }

        let data_base = unsafe { self.base.add(CQ_DATA_OFFSET) };
        unsafe {
            *data_base.add(tail % self.capacity) = cmd_type;
            *data_base.add((tail + 1) % self.capacity) = payload.len() as u8;
        }
        for (index, byte) in payload.iter().copied().enumerate() {
            unsafe {
                *data_base.add((tail + 2 + index) % self.capacity) = byte;
            }
        }

        self.tail_atomic()
            .store((tail + total) as u32, Ordering::Release);
        true
    }

    fn head_atomic(&self) -> &AtomicU32 {
        unsafe { &*self.base.add(CQ_HEAD_OFFSET).cast::<AtomicU32>() }
    }

    fn tail_atomic(&self) -> &AtomicU32 {
        unsafe { &*self.base.add(CQ_TAIL_OFFSET).cast::<AtomicU32>() }
    }
}
