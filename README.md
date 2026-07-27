# LOBOT Lab

[![CI](https://github.com/tais/lobot-lab/actions/workflows/ci.yml/badge.svg)](https://github.com/tais/lobot-lab/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

LOBOT Lab is a native Tauri/Rust workbench for inspecting and testing
Jagged Alliance 2 v1.13 Logical Body Type (LOBOT) configurations. It loads the
game's XML, STI, palette, and SLF data; builds a test soldier from a merc profile
and inventory; renders the resolved layers; and explains why each surface was
selected.

The project is in early development. It is useful for diagnosing LOBOT data, but
the preview is not a replacement for final in-game testing.

## Features

- Discovers active Data directories from a v1.13 `vfs_config*.ini`.
- Resolves loose-file overlays and classic SLF libraries from low to high
  priority.
- Loads characters, items, attachments, armour, LBE gear, filters, palettes,
  logical body types, layers, and animation surfaces.
- Restricts inventory choices to items accepted by the selected slot.
- Provides searchable, scrollable item and attachment pickers.
- Derives handgun, dual-handgun, rifle, unarmed, water, injury, and big-merc
  animation variants from the test soldier's state.
- Applies profile hair, skin, vest, and pants colours plus equipment, applied
  camouflage, stealth, and per-layer palettes.
- Decodes indexed ETRLE STI files and companion alpha surfaces.
- Supports every facing direction, frame playback, integer zoom, and several
  preview backgrounds.
- Traces the selected filter, surface, palette, z value, sprite direction, and
  STI subimage for every layer.
- Audits missing XML entities, references, palettes, files, frames, and alpha
  companions.

## Requirements

- Node.js LTS and npm
- Rust 1.85 or newer
- The [Tauri v2 platform prerequisites](https://v2.tauri.app/start/prerequisites/)
- A legally obtained Jagged Alliance 2 installation with v1.13 data

LOBOT Lab does not include or download JA2 executables, SLF archives, game data,
or mod assets.

## Development

```sh
git clone https://github.com/tais/lobot-lab.git
cd lobot-lab
npm ci
npm run tauri dev
```

## Loading game data

1. Open **Data roots** and choose the merged JA2 v1.13 installation directory.
2. Confirm that the base `Data` directory appears before `Data-1.13`.
3. Add each mod Data directory after those roots. Later entries have higher
   priority.
4. Load the workspace, select a character, equip items, and choose an action.

A conventional root order is:

```text
<JA2 install>/Data
<JA2 install>/Data-1.13
<optional mod>/Data-MyMod
```

The base installation should contain the original JA2 SLF archives, including
`Data/Anims.slf`. A v1.13 data package copied beside the original game rather
than over it is not a complete runtime installation.

## Checks and builds

```sh
npm run check
npm run build
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

Create native application bundles with:

```sh
npm run tauri build
```

Unsigned local and GitHub release builds may produce operating-system security
warnings. Production distribution should use the appropriate macOS and Windows
code-signing credentials.

### Optional real-data integration checks

The Rust suite always runs without proprietary data. To additionally exercise a
local v1.13 installation and optional mod overlay, set:

```sh
LOBOT_TEST_INSTALL="/path/to/JA2-1.13" \
LOBOT_TEST_OVERLAY="/path/to/mod-data" \
cargo test --manifest-path src-tauri/Cargo.toml
```

These paths are read only at test time and are never stored by the project.

### Regenerating the animation catalogs

The checked-in catalogs are derived from the public
[JA2 v1.13 source](https://github.com/1dot13/source). To regenerate them after
an upstream animation change:

```sh
npm run catalog -- /path/to/ja2-v1.13-source
```

Alternatively, set `JA2_SOURCE_ROOT` and run `npm run catalog`.

## Scope and fidelity

LOBOT Lab reproduces logical layer selection, direction/frame mapping, palettes,
sprite placement, and alpha composition in a neutral preview. Tactical-world
lighting, glow tables, obscured blitters, and z-buffer occlusion remain
game-renderer concerns.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for implementation details and
[CONTRIBUTING.md](./CONTRIBUTING.md) before submitting a change.

## Legal

LOBOT Lab is an independent community tool and is not affiliated with or
endorsed by Strategy First, Sir-Tech, or the JA2 v1.13 maintainers. Jagged
Alliance and related names and assets belong to their respective owners. This
repository contains no proprietary game data.

LOBOT Lab's own source code is available under the [MIT License](./LICENSE).
