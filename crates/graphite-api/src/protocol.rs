pub const OFFSET_TICK_COUNTER: usize = 0x0000;
pub const OFFSET_SNAPSHOT_READY: usize = 0x0008;
pub const OFFSET_COMMAND_COUNT: usize = 0x000C;
pub const OFFSET_WORLD_SNAPSHOT: usize = 0x0040;
pub const OFFSET_COMMAND_QUEUE: usize = 0x30000;
pub const OFFSET_EVENT_RING: usize = 0x38000;

pub const SNAP_ENTITY_COUNT: usize = 0;
pub const SNAP_CHUNK_SECTION_COUNT: usize = 4;
pub const SNAP_TIMESTAMP_NS: usize = 8;
pub const SNAP_VERSION: usize = 16;
pub const SNAP_FLAGS: usize = 20;
pub const SNAP_ENTITY_DATA_SIZE: usize = 24;
pub const SNAP_CHUNK_DATA_SIZE: usize = 28;
pub const SNAP_HEADER_SIZE: usize = 32;

pub const ENTITY_RECORD_SIZE: usize = 48;
pub const CHUNK_SECTION_RECORD_SIZE: usize = 16400;

pub const MAX_ENTITIES: usize = 4096;
pub const MAX_CHUNK_SECTIONS: usize = 24;

pub const CQ_HEAD_OFFSET: usize = 0;
pub const CQ_TAIL_OFFSET: usize = 4;
pub const CQ_CAPACITY_OFFSET: usize = 8;
pub const CQ_DATA_OFFSET: usize = 16;
pub const CQ_CAPACITY: usize = 0x7FF0 - CQ_DATA_OFFSET;

pub const CMD_SET_BLOCK: u8 = 0x01;
pub const CMD_SEND_CHAT: u8 = 0x02;
pub const CMD_SPAWN_PARTICLE: u8 = 0x03;
pub const CMD_SET_VELOCITY: u8 = 0x04;
pub const CMD_KILL_ENTITY: u8 = 0x05;

pub const ENTITY_KIND_UNKNOWN: u16 = 0;
pub const ENTITY_KIND_PLAYER: u16 = 1;
pub const ENTITY_KIND_MOB: u16 = 2;
pub const ENTITY_KIND_ITEM: u16 = 3;
pub const ENTITY_KIND_PROJECTILE: u16 = 4;

pub const ENTITY_FLAG_ALIVE: u16 = 1 << 0;
pub const ENTITY_FLAG_ON_GROUND: u16 = 1 << 1;
pub const ENTITY_FLAG_IN_WATER: u16 = 1 << 2;
pub const ENTITY_FLAG_IS_SPRINTING: u16 = 1 << 3;

pub const PROTOCOL_VERSION: u32 = 2;
