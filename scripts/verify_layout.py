#!/usr/bin/env python3
import dataclasses
import re
import sys
from pathlib import Path


@dataclasses.dataclass
class Constant:
    name: str
    value: int
    file: str
    line: int


def parse_java(path: Path) -> dict[str, Constant]:
    consts = {}
    int_pat = re.compile(r"(?:public\s+)?(?:static\s+)?(?:final\s+)?int\s+(\w+)\s*=\s*(0x[0-9a-fA-F]+|-?\d+)")
    byte_pat = re.compile(r"(?:public\s+)?(?:static\s+)?(?:final\s+)?byte\s+(\w+)\s*=\s*(0x[0-9a-fA-F]+|-?\d+)")
    for pattern in (int_pat, byte_pat):
        for index, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            match = pattern.search(line)
            if match:
                consts[match.group(1)] = Constant(match.group(1), int(match.group(2), 0), str(path), index)
    return consts


def parse_rust(path: Path) -> dict[str, Constant]:
    consts = {}
    pattern = re.compile(r"pub\s+const\s+(\w+)\s*:\s*\w+\s*=\s*(0x[0-9a-fA-F]+|-?\d+)")
    for index, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        match = pattern.search(line)
        if match:
            consts[match.group(1)] = Constant(match.group(1), int(match.group(2), 0), str(path), index)
    return consts


def verify(java_files: list[Path], rust_files: list[Path]) -> int:
    java_consts = {}
    rust_consts = {}

    for path in java_files:
        java_consts.update(parse_java(path))
    for path in rust_files:
        rust_consts.update(parse_rust(path))

    must_match = [
        ("OFFSET_TICK_COUNTER", "OFFSET_TICK_COUNTER"),
        ("OFFSET_SNAPSHOT_READY", "OFFSET_SNAPSHOT_READY"),
        ("OFFSET_COMMAND_COUNT", "OFFSET_COMMAND_COUNT"),
        ("OFFSET_WORLD_SNAPSHOT", "OFFSET_WORLD_SNAPSHOT"),
        ("OFFSET_COMMAND_QUEUE", "OFFSET_COMMAND_QUEUE"),
        ("OFFSET_EVENT_RING", "OFFSET_EVENT_RING"),
        ("SNAP_ENTITY_COUNT", "SNAP_ENTITY_COUNT"),
        ("SNAP_CHUNK_SEC_COUNT", "SNAP_CHUNK_SECTION_COUNT"),
        ("SNAP_TIMESTAMP_NS", "SNAP_TIMESTAMP_NS"),
        ("SNAP_VERSION", "SNAP_VERSION"),
        ("SNAP_FLAGS", "SNAP_FLAGS"),
        ("SNAP_ENTITY_DATA_SIZE", "SNAP_ENTITY_DATA_SIZE"),
        ("SNAP_CHUNK_DATA_SIZE", "SNAP_CHUNK_DATA_SIZE"),
        ("SNAP_HEADER_SIZE", "SNAP_HEADER_SIZE"),
        ("ENTITY_RECORD_SIZE", "ENTITY_RECORD_SIZE"),
        ("CHUNK_SECTION_RECORD_SIZE", "CHUNK_SECTION_RECORD_SIZE"),
        ("MAX_ENTITIES", "MAX_ENTITIES"),
        ("MAX_CHUNK_SECTIONS", "MAX_CHUNK_SECTIONS"),
        ("PROTOCOL_VERSION", "PROTOCOL_VERSION"),
        ("CQ_HEAD_OFFSET", "CQ_HEAD_OFFSET"),
        ("CQ_TAIL_OFFSET", "CQ_TAIL_OFFSET"),
        ("CQ_CAPACITY_OFFSET", "CQ_CAPACITY_OFFSET"),
        ("CQ_DATA_OFFSET", "CQ_DATA_OFFSET"),
        ("SET_BLOCK", "CMD_SET_BLOCK"),
        ("SEND_CHAT", "CMD_SEND_CHAT"),
        ("SPAWN_PARTICLE", "CMD_SPAWN_PARTICLE"),
        ("SET_VELOCITY", "CMD_SET_VELOCITY"),
        ("KILL_ENTITY", "CMD_KILL_ENTITY"),
    ]

    errors = []
    warnings = []
    ok_count = 0

    for java_name, rust_name in must_match:
        jc = java_consts.get(java_name)
        rc = rust_consts.get(rust_name)
        if jc is None and rc is None:
            warnings.append(f"WARN: missing on both sides: {java_name}")
        elif jc is None:
            errors.append(f"MISSING in Java: {rust_name} = 0x{rc.value:x} ({rc.file}:{rc.line})")
        elif rc is None:
            errors.append(f"MISSING in Rust: {java_name} = 0x{jc.value:x} ({jc.file}:{jc.line})")
        elif jc.value != rc.value:
            errors.append(
                f"MISMATCH: {java_name}\n"
                f"  Java: 0x{jc.value:04x} ({jc.file}:{jc.line})\n"
                f"  Rust: 0x{rc.value:04x} ({rc.file}:{rc.line})"
            )
        else:
            ok_count += 1

    for name, expected in (("ENTITY_RECORD_SIZE", 48), ("CHUNK_SECTION_RECORD_SIZE", 16400)):
        rc = rust_consts.get(name)
        if rc is not None:
            if rc.value != expected:
                errors.append(f"STRUCT SIZE mismatch for {name}: expected {expected}, got {rc.value}")
            else:
                ok_count += 1

    if warnings:
        print("Warnings:")
        for warning in warnings:
            print(warning)

    if errors:
        print(f"Protocol verification failed with {len(errors)} error(s):")
        for error in errors:
            print(error)
        return 1

    print(f"Protocol verified: {ok_count} constants matched")
    return 0


if __name__ == "__main__":
    if len(sys.argv) < 4 or "--" not in sys.argv:
        print(f"Usage: {sys.argv[0]} <java_files...> -- <rust_files...>")
        sys.exit(1)

    sep = sys.argv.index("--")
    java_files = [Path(arg) for arg in sys.argv[1:sep]]
    rust_files = [Path(arg) for arg in sys.argv[sep + 1 :]]
    sys.exit(verify(java_files, rust_files))
