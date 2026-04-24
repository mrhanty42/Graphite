# Architecture

## Components

### `graphite-host`

NeoForge host mod responsible for:

- loading the native runtime
- allocating off-heap shared memory
- serializing world state into the snapshot buffer
- draining Rust-written commands and applying them to the server world

### `graphite-core`

Native runtime responsible for:

- JNI bridge functions
- tick loop
- shared-memory reads/writes
- dynamic loading of Rust gameplay modules

### `graphite-api`

Shared Rust crate that defines:

- binary protocol constants
- zero-copy `WorldView`
- `CommandQueue`
- C ABI for external Graphite mods

### `graphite-test-mod`

A demo module proving the vertical slice works:

- loaded by `graphite-core`
- reads player state from `WorldView`
- writes chat and particle commands into the command queue

## Tick Pipeline

1. `GraphiteHost` receives `LevelTickEvent.Pre`.
2. `WorldSnapshotWriter` writes the current snapshot into direct memory.
3. Java calls `NativeBridge.graphiteTick(tickId)`.
4. `graphite-core` wakes the tick thread.
5. `graphite-core` invokes all loaded Graphite mods.
6. Mods read world data and write commands to the command queue.
7. `GraphiteHost` receives `LevelTickEvent.Post`.
8. `CommandQueueReader` drains the queue and applies game actions.

## Shared Memory Layout

The authoritative protocol constants live in:

- [`crates/graphite-api/src/protocol.rs`](../crates/graphite-api/src/protocol.rs)

The Java and Rust sides must stay aligned. CI validates this with:

- [`scripts/verify_layout.py`](../scripts/verify_layout.py)

## Runtime Model

- One direct shared-memory region allocated by Java
- One JNI tick signal in the hot path
- One Rust tick thread processing snapshot data
- One command queue written by Rust and consumed by Java

## Current Dev Notes

- Local NeoForge runs use `graphite-host/runs/client`
- Local demo mods are copied into `runs/client/graphite_mods`
- `prepareRun` is the intended local bootstrap task
