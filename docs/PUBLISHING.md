# Publishing Checklist

## Before pushing

- Run `cargo check` in `crates/`
- Run `cargo build --release -p graphite-core`
- Run `cargo build --release -p graphite-test-mod`
- Run `graphite-host\gradlew.bat build --no-daemon`
- Run `scripts/verify_layout.py`

## Do not commit

- `crates/target/`
- `graphite-host/build/`
- `graphite-host/run/`
- `graphite-host/runs/`
- copied local native binaries in `graphite-host/src/main/resources/natives/*`

## Repository contents that should remain

- source code
- Gradle wrapper
- CI workflow
- documentation
- validation scripts

## Suggested first GitHub repo description

`Experimental Rust runtime for Minecraft NeoForge with shared-memory world snapshots and hot-loadable Rust mods.`
