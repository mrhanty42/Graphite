# Graphite

Graphite is an experimental Rust runtime for Minecraft on NeoForge. It embeds a native `graphite-core` library into a Java host mod, shares world state through off-heap memory, and lets external Rust mods react to ticks and write commands back into the game.

Current state:

- Java host mod for NeoForge `1.21.1`
- Rust runtime with JNI bridge
- Shared-memory `WorldSnapshot`
- Rust-side mod loader with hot-reload entrypoints
- Demo Rust mod that proves the pipeline with chat + particle commands

This repository is a prototype, not a stable public API yet.

## Repository Layout

```text
.
├── crates/
│   ├── graphite-api/       # Shared protocol, world view, command queue, mod ABI
│   ├── graphite-core/      # Native runtime loaded by the Java host
│   └── graphite-test-mod/  # Demo Rust mod used for end-to-end validation
├── graphite-host/          # NeoForge host mod
├── scripts/                # Validation helpers
└── .github/workflows/      # CI
```

## Requirements

- Java 21
- Rust stable
- Windows, Linux, or macOS

For local NeoForge development, Graphite currently assumes:

- Minecraft `1.21.1`
- NeoForge `21.1.77`

## Quick Start

### 1. Build Rust artifacts

```powershell
cd crates
cargo build --release -p graphite-core
cargo build --release -p graphite-test-mod
```

### 2. Prepare the NeoForge dev run

```powershell
cd ..\graphite-host
.\gradlew.bat prepareRun --no-daemon
```

This does three things:

- builds the Java host mod
- copies `graphite_core` into host resources for local development
- copies `graphite_test_mod` into `runs/client/graphite_mods`

### 3. Launch the client

```powershell
.\gradlew.bat runClient --no-daemon
```

Expected result:

- the host mod loads
- the Rust runtime initializes
- the demo Rust mod is discovered and loaded
- entering a world shows a one-time chat message and a ring of flame particles around the player

## Development Workflow

Useful commands:

```powershell
# Rust
cd crates
cargo check
cargo build --release -p graphite-core
cargo build --release -p graphite-test-mod

# Layout verification
python ..\scripts\verify_layout.py `
  ..\graphite-host\src\main\java\dev\graphite\host\SharedMemory.java `
  ..\graphite-host\src\main\java\dev\graphite\host\commands\CommandType.java `
  ..\graphite-host\src\main\java\dev\graphite\host\snapshot\WorldSnapshotWriter.java `
  -- `
  graphite-api\src\protocol.rs

# Host mod
cd ..\graphite-host
.\gradlew.bat build --no-daemon
.\gradlew.bat prepareRun --no-daemon
.\gradlew.bat runClient --no-daemon
```

## Architecture

High-level pipeline:

1. Java writes a world snapshot into shared direct memory.
2. Java signals the native runtime with one JNI call per tick.
3. Rust reads the snapshot through `graphite-api::WorldView`.
4. Rust mods write gameplay commands into a command queue.
5. Java drains that queue at the end of the level tick and applies the commands.

More detail is in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## CI

The workflow in [.github/workflows/build.yml](.github/workflows/build.yml) builds:

- `graphite-core`
- `graphite-test-mod`
- the Java host mod
- protocol layout verification

## Known Limitations

- The binary protocol is still evolving.
- Block and particle registries are not finalized yet.
- The demo mod is for validation, not gameplay.
- The hot-reload path is still developer-focused.

## License

MIT. See [LICENSE](LICENSE).
