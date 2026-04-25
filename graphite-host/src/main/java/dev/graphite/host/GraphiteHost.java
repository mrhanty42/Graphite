package dev.graphite.host;

import com.mojang.brigadier.Command;
import com.mojang.brigadier.arguments.StringArgumentType;
import dev.graphite.host.commands.CommandQueueReader;
import dev.graphite.host.snapshot.WorldSnapshotWriter;
import net.minecraft.commands.Commands;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.fml.loading.FMLPaths;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.RegisterCommandsEvent;
import net.neoforged.neoforge.event.server.ServerStoppingEvent;
import net.neoforged.neoforge.event.tick.LevelTickEvent;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.nio.file.Files;
import java.nio.file.Path;

@Mod(GraphiteHost.MODID)
public class GraphiteHost {
    public static final String MODID = "graphite_host";
    public static final Logger LOG = LoggerFactory.getLogger(MODID);
    public static final Path MODS_DIR = FMLPaths.GAMEDIR.get().resolve("graphite_mods");

    private static SharedMemory sharedMem;
    private static WorldSnapshotWriter snapshotWriter;
    private static CommandQueueReader commandReader;
    private static long tickCounter = 0;
    private static boolean runtimeReady = false;
    private static final boolean DEBUG_LOGGING = Boolean.getBoolean("graphite.debug");

    public GraphiteHost(IEventBus modBus) {
        modBus.addListener(this::onSetup);
        NeoForge.EVENT_BUS.register(this);
    }

    private void onSetup(FMLCommonSetupEvent event) {
        event.enqueueWork(() -> {
            try {
                if (DEBUG_LOGGING) {
                    LOG.info("[Graphite] ===== PATH DIAGNOSTICS =====");
                    LOG.info("[Graphite] GAMEDIR  = {}", FMLPaths.GAMEDIR.get().toAbsolutePath());
                    LOG.info("[Graphite] user.dir = {}", System.getProperty("user.dir"));
                    LOG.info("[Graphite] MODS_DIR = {}", MODS_DIR.toAbsolutePath());
                    LOG.info("[Graphite] ============================");
                }

                NativeLoader.load();
                LOG.info("[Graphite] Native library loaded");

                sharedMem = new SharedMemory();
                snapshotWriter = new WorldSnapshotWriter(sharedMem.getStateBuffer());
                commandReader = new CommandQueueReader(sharedMem.getStateBuffer());

                Files.createDirectories(MODS_DIR);
                LOG.info("[Graphite] Mods directory: {}", MODS_DIR);
                if (DEBUG_LOGGING) {
                    LOG.info("[Graphite] Mods directory exists: {}", Files.exists(MODS_DIR));
                    LOG.info("[Graphite] Mods directory contents:");
                    try (var stream = Files.list(MODS_DIR)) {
                        stream.forEach(path -> LOG.info("[Graphite]   {}", path.getFileName()));
                    }
                }

                NativeBridge.graphiteInit(
                    sharedMem.getStateAddress(),
                    sharedMem.getStateSize(),
                    sharedMem.getEventRingAddress(),
                    sharedMem.getEventRingSize(),
                    MODS_DIR.toAbsolutePath().toString()
                );

                runtimeReady = true;
                LOG.info("[Graphite] Runtime ready: {}", NativeBridge.graphiteDebugInfo());
            } catch (Exception e) {
                LOG.error("[Graphite] Critical initialization error, Rust mods disabled", e);
            }
        });
    }

    @SubscribeEvent
    public void onRegisterCommands(RegisterCommandsEvent event) {
        event.getDispatcher().register(
            Commands.literal("graphite")
                .requires(source -> source.hasPermission(2))
                .then(Commands.literal("status")
                    .executes(ctx -> {
                        String info = runtimeReady
                            ? NativeBridge.graphiteDebugInfo()
                            : "Runtime not initialized";
                        ctx.getSource().sendSuccess(() -> Component.literal("[Graphite] " + info), false);
                        return Command.SINGLE_SUCCESS;
                    }))
                .then(Commands.literal("reload")
                    .then(Commands.argument("file", StringArgumentType.string())
                        .executes(ctx -> {
                            if (!runtimeReady) {
                                ctx.getSource().sendFailure(Component.literal("[Graphite] Runtime not ready"));
                                return 0;
                            }

                            String file = StringArgumentType.getString(ctx, "file");
                            Path modsRoot = MODS_DIR.toAbsolutePath().normalize();
                            Path libPath = MODS_DIR.resolve(file).toAbsolutePath().normalize();
                            if (!libPath.startsWith(modsRoot)) {
                                ctx.getSource().sendFailure(Component.literal(
                                    "[Graphite] Invalid path: file must be inside " + MODS_DIR
                                ));
                                return 0;
                            }
                            boolean accepted = NativeBridge.graphiteReloadMod(libPath.toString());

                            if (accepted) {
                                ctx.getSource().sendSuccess(
                                    () -> Component.literal("[Graphite] Reload requested: " + file),
                                    true
                                );
                                return Command.SINGLE_SUCCESS;
                            }

                            ctx.getSource().sendFailure(Component.literal("[Graphite] Reload request failed"));
                            return 0;
                        })))
        );
    }

    @SubscribeEvent
    public void onLevelTickStart(LevelTickEvent.Pre event) {
        if (!runtimeReady) {
            return;
        }
        if (!(event.getLevel() instanceof ServerLevel level)) {
            return;
        }

        // Check if Rust has finished processing the previous snapshot.
        // Java is the only writer of SNAPSHOT_READY=1, Rust is the only writer of SNAPSHOT_READY=0,
        // so a simple volatile check is sufficient (no CAS needed).
        if (SharedMemory.getIntVolatile(sharedMem.getStateBuffer(), SharedMemory.OFFSET_SNAPSHOT_READY) != 0) {
            return;
        }

        // We own the snapshot buffer, safe to write
        tickCounter++;
        snapshotWriter.write(level, tickCounter);
        NativeBridge.graphiteTick(tickCounter);
    }

    @SubscribeEvent
    public void onLevelTickEnd(LevelTickEvent.Post event) {
        if (!runtimeReady) {
            return;
        }
        if (!(event.getLevel() instanceof ServerLevel level)) {
            return;
        }

        commandReader.drain(level);
    }

    @SubscribeEvent
    public void onServerStopping(ServerStoppingEvent event) {
        if (runtimeReady) {
            LOG.info("[Graphite] Shutting down Rust runtime");
            NativeBridge.graphiteShutdown();
        }
    }

    public static SharedMemory sharedMem() {
        return sharedMem;
    }
}
