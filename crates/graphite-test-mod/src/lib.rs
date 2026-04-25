use graphite_api::{
    commands::CommandQueue,
    mod_trait::{GraphiteModImpl, ModLoadContext},
    world::WorldView,
};
use crate::effects::ParticleEffects;

mod effects;

pub struct DiagnosticMod {
    ticks_processed: u64,
    last_log_tick: u64,
    announced_world: bool,
}

impl GraphiteModImpl for DiagnosticMod {
    fn new() -> Self {
        Self {
            ticks_processed: 0,
            last_log_tick: 0,
            announced_world: false,
        }
    }

    fn on_load(&mut self, ctx: *const ModLoadContext) {
        let proto_version = unsafe { (*ctx).protocol_version };
        let entity_size = unsafe { (*ctx).entity_record_size };

        log::info!("[diagnostic] graphite-diagnostic-mod loaded");
        log::info!("[diagnostic] protocol version: {} (expected {})", proto_version, graphite_api::protocol::PROTOCOL_VERSION);
        log::info!("[diagnostic] entity record size: {} bytes (expected {})", entity_size, graphite_api::protocol::ENTITY_RECORD_SIZE as u32);

        if proto_version != graphite_api::protocol::PROTOCOL_VERSION {
            log::error!(
                "[diagnostic] Protocol version mismatch: got {}, expected {}",
                proto_version,
                graphite_api::protocol::PROTOCOL_VERSION
            );
            return;
        }
        if entity_size != graphite_api::protocol::ENTITY_RECORD_SIZE as u32 {
            log::error!(
                "[diagnostic] EntityRecord size mismatch: got {}, expected {}",
                entity_size,
                graphite_api::protocol::ENTITY_RECORD_SIZE as u32
            );
            return;
        }

        log::info!("[diagnostic] all checks passed");
    }

    fn on_tick(&mut self, world: &WorldView, cmd: &mut CommandQueue, tick: u64) {
        self.ticks_processed += 1;

        if !self.announced_world && world.players().next().is_some() {
            let _ = cmd.send_chat(u32::MAX, "[Graphite] Rust mod active");
            self.announced_world = true;
        }

        if tick % 20 == 0 {
            for player in world.players() {
                ParticleEffects::ring(cmd, player.x(), player.y() + 1.0, player.z(), 1.75, 8, 0);
            }
        }

        if tick % 100 == 0 {
            let _ = cmd.send_chat(
                u32::MAX,
                &format!(
                    "[Graphite] tick={} players={} entities={}",
                    tick,
                    world.players().count(),
                    world.entity_count()
                ),
            );
        }

        if tick.saturating_sub(self.last_log_tick) < 100 {
            return;
        }
        self.last_log_tick = tick;

        let entity_count = world.entity_count();
        let player_count = world.players().count();
        let timestamp_ms = world.timestamp_ns() / 1_000_000;

        log::info!(
            "[diagnostic] tick={} | entities={} | players={} | snapshot_age={}ms | processed={}",
            tick,
            entity_count,
            player_count,
            timestamp_ms,
            self.ticks_processed
        );

        for (index, player) in world.players().enumerate() {
            log::info!(
                "[diagnostic] player[{}]: id={} pos=({:.1},{:.1},{:.1}) hp={:.1} flags={:04b}",
                index,
                player.entity_id(),
                player.x(),
                player.y(),
                player.z(),
                player.health(),
                player.flags()
            );
        }
    }

    fn on_unload(&mut self) {
        log::info!(
            "[diagnostic] unloaded after {} processed ticks",
            self.ticks_processed
        );
    }
}

graphite_api::graphite_mod! {
    name: "graphite-diagnostic",
    version: "0.1.0",
    type: DiagnosticMod,
}
