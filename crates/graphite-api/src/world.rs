use crate::protocol::*;
use bytemuck::{Pod, Zeroable};
use std::marker::PhantomData;

#[repr(C, packed)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct EntityRecord {
    pub entity_id: i32,
    pub kind: u16,
    pub flags: u16,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub health: f32,
}

const _: [(); ENTITY_RECORD_SIZE] = [(); std::mem::size_of::<EntityRecord>()];

impl EntityRecord {
    pub fn entity_id(&self) -> i32 {
        unsafe { std::ptr::addr_of!(self.entity_id).read_unaligned() }
    }

    pub fn kind(&self) -> u16 {
        unsafe { std::ptr::addr_of!(self.kind).read_unaligned() }
    }

    pub fn flags(&self) -> u16 {
        unsafe { std::ptr::addr_of!(self.flags).read_unaligned() }
    }

    pub fn x(&self) -> f64 {
        unsafe { std::ptr::addr_of!(self.x).read_unaligned() }
    }

    pub fn y(&self) -> f64 {
        unsafe { std::ptr::addr_of!(self.y).read_unaligned() }
    }

    pub fn z(&self) -> f64 {
        unsafe { std::ptr::addr_of!(self.z).read_unaligned() }
    }

    pub fn health(&self) -> f32 {
        unsafe { std::ptr::addr_of!(self.health).read_unaligned() }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct ChunkSectionHeader {
    pub chunk_x: i32,
    pub section_y: i32,
    pub chunk_z: i32,
    pub flags: u32,
}

const _: [(); 16] = [(); std::mem::size_of::<ChunkSectionHeader>()];

pub struct WorldView<'a> {
    snapshot_base: *const u8,
    entity_count: u32,
    chunk_count: u32,
    _lifetime: PhantomData<&'a ()>,
}

unsafe impl<'a> Send for WorldView<'a> {}

impl<'a> WorldView<'a> {
    pub unsafe fn from_raw(base: *const u8) -> Self {
        let entity_count = unsafe { base.add(SNAP_ENTITY_COUNT).cast::<u32>().read_unaligned() };
        let chunk_count =
            unsafe { base.add(SNAP_CHUNK_SECTION_COUNT).cast::<u32>().read_unaligned() };
        let version = unsafe { base.add(SNAP_VERSION).cast::<u32>().read_unaligned() };

        assert_eq!(
            version, PROTOCOL_VERSION,
            "Protocol version mismatch: Java={version}, Rust={PROTOCOL_VERSION}"
        );

        Self {
            snapshot_base: base,
            entity_count: entity_count.min(MAX_ENTITIES as u32),
            chunk_count: chunk_count.min(MAX_CHUNK_SECTIONS as u32),
            _lifetime: PhantomData,
        }
    }

    #[inline]
    pub fn entity_count(&self) -> usize {
        self.entity_count as usize
    }

    #[inline]
    pub fn chunk_count(&self) -> usize {
        self.chunk_count as usize
    }

    pub fn entities(&self) -> &[EntityRecord] {
        let ptr = unsafe { self.snapshot_base.add(SNAP_HEADER_SIZE) as *const EntityRecord };
        unsafe { std::slice::from_raw_parts(ptr, self.entity_count()) }
    }

    pub fn players(&self) -> impl Iterator<Item = &EntityRecord> {
        self.entities()
            .iter()
            .filter(|entity| entity.kind() == ENTITY_KIND_PLAYER)
    }

    pub fn living_entities(&self) -> impl Iterator<Item = &EntityRecord> {
        self.entities().iter().filter(|entity| {
            entity.flags() & ENTITY_FLAG_ALIVE != 0
                && matches!(entity.kind(), ENTITY_KIND_PLAYER | ENTITY_KIND_MOB)
        })
    }

    pub fn get_block_state_id(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        let chunk_x = x >> 4;
        let chunk_z = z >> 4;
        let section_y = y >> 4;

        let section = self.find_chunk_section(chunk_x, section_y, chunk_z)?;
        let lx = (x & 0xF) as usize;
        let ly = (y & 0xF) as usize;
        let lz = (z & 0xF) as usize;
        let idx = (ly << 8) | (lz << 4) | lx;

        let block_data_offset = SNAP_HEADER_SIZE
            + self.entity_count() * ENTITY_RECORD_SIZE
            + section * CHUNK_SECTION_RECORD_SIZE
            + std::mem::size_of::<ChunkSectionHeader>();

        let state_id = unsafe {
            self.snapshot_base
                .add(block_data_offset + idx * 2)
                .cast::<u16>()
                .read_unaligned()
        };

        Some(state_id)
    }

    pub fn timestamp_ns(&self) -> u64 {
        unsafe {
            self.snapshot_base
                .add(SNAP_TIMESTAMP_NS)
                .cast::<u64>()
                .read_unaligned()
        }
    }

    fn find_chunk_section(&self, cx: i32, sy: i32, cz: i32) -> Option<usize> {
        let sections_start = SNAP_HEADER_SIZE + self.entity_count() * ENTITY_RECORD_SIZE;

        for index in 0..self.chunk_count() {
            let offset = sections_start + index * CHUNK_SECTION_RECORD_SIZE;
            let header = unsafe {
                self.snapshot_base
                    .add(offset)
                    .cast::<ChunkSectionHeader>()
                    .read_unaligned()
            };
            if header.chunk_x == cx && header.section_y == sy && header.chunk_z == cz {
                return Some(index);
            }
        }

        None
    }
}
