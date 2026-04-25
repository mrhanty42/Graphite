use crate::protocol::*;
use std::marker::PhantomData;

#[repr(C, packed)]
#[derive(Copy, Clone)]
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

const _: () = assert!(std::mem::size_of::<EntityRecord>() == ENTITY_RECORD_SIZE);

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

    pub fn vx(&self) -> f32 {
        unsafe { std::ptr::addr_of!(self.vx).read_unaligned() }
    }

    pub fn vy(&self) -> f32 {
        unsafe { std::ptr::addr_of!(self.vy).read_unaligned() }
    }

    pub fn vz(&self) -> f32 {
        unsafe { std::ptr::addr_of!(self.vz).read_unaligned() }
    }

    pub fn health(&self) -> f32 {
        unsafe { std::ptr::addr_of!(self.health).read_unaligned() }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct ChunkSectionHeader {
    pub chunk_x: i32,
    pub section_y: i32,
    pub chunk_z: i32,
    pub flags: u32,
}

impl ChunkSectionHeader {
    pub fn chunk_x(&self) -> i32 {
        unsafe { std::ptr::addr_of!(self.chunk_x).read_unaligned() }
    }

    pub fn section_y(&self) -> i32 {
        unsafe { std::ptr::addr_of!(self.section_y).read_unaligned() }
    }

    pub fn chunk_z(&self) -> i32 {
        unsafe { std::ptr::addr_of!(self.chunk_z).read_unaligned() }
    }

    pub fn flags(&self) -> u32 {
        unsafe { std::ptr::addr_of!(self.flags).read_unaligned() }
    }
}

const _: () = assert!(std::mem::size_of::<ChunkSectionHeader>() == 16);

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

        if version != PROTOCOL_VERSION {
            log::error!(
                "[Graphite] Snapshot version mismatch: got {}, expected {} — skipping tick",
                version,
                PROTOCOL_VERSION
            );
            // Return an empty view instead of panicking
            return Self {
                snapshot_base: base,
                entity_count: 0,
                chunk_count: 0,
                _lifetime: PhantomData,
            };
        }

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

    pub fn entities(&self) -> impl Iterator<Item = EntityRecord> + '_ {
        let base = self.snapshot_base;
        (0..self.entity_count()).map(move |index| unsafe {
            let off = SNAP_HEADER_SIZE + index * ENTITY_RECORD_SIZE;
            base.add(off).cast::<EntityRecord>().read_unaligned()
        })
    }

    pub fn players(&self) -> impl Iterator<Item = EntityRecord> + '_ {
        self.entities()
            .filter(|entity| entity.kind() == ENTITY_KIND_PLAYER)
    }

    pub fn living_entities(&self) -> impl Iterator<Item = EntityRecord> + '_ {
        self.entities().filter(|entity| {
            entity.flags() & ENTITY_FLAG_ALIVE != 0
                && matches!(entity.kind(), ENTITY_KIND_PLAYER | ENTITY_KIND_MOB)
        })
    }

    pub fn get_block_state_id(&self, x: i32, y: i32, z: i32) -> Option<u32> {
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
                .add(block_data_offset + idx * 4)
                .cast::<u32>()
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
            if header.chunk_x() == cx && header.section_y() == sy && header.chunk_z() == cz {
                return Some(index);
            }
        }

        None
    }
}
