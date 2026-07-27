# Architecture

## Data flow

```text
ordered Data roots + SLF libraries
  → case-insensitive virtual file resolver
  → XML entity expansion
  → LOBOT + profile/item model
  → test SOLDIERTYPE state + indexed attachments + scenario
  → animation-state / equipment-variant resolution
  → first-match filter/layer resolution
  → STI/alpha decode + profile/equipment camouflage/item palette selection
  → offset-aware RGBA composite
```

The Rust backend owns parsing, selection, rendering, and completeness auditing.
The Svelte frontend sends a character/inventory/attachments/scenario/action/
direction/frame request and displays the composite plus the backend's
resolution trace.

## Correspondence with JA2 1.13

The implementation was traced against the public
[JA2 v1.13 source](https://github.com/1dot13/source):

- `Ja2/Init.cpp` defines the LOBOT load order:
  Layers → Palettes → Animation Surfaces → Filters → Logical Body Types.
- `Tactical/LogicalBodyTypes/BodyType.h` tries animation-state mappings first,
  then physical-animation-surface mappings, and returns the first matching
  filter for a layer.
- `Tactical/LogicalBodyTypes/Filter.cpp` supplies the soldier/inventory fields
  and its AND/OR behavior.
- `Tactical/Animation Control.cpp::DetermineSoldierAnimationSurface` selects
  handgun, valid dual-handgun, unarmed, rifle, water, injured-walk, and big-merc
  physical surfaces for an animation state.
- `Tactical/Items.cpp::GetWornCamo` and its urban/desert/snow/stealth siblings
  combine armour, LBE, weapons, and attachments into the palette decision.
- `Tactical/Soldier Control.cpp::CreateSoldierPalettes` and
  `SetPaletteReplacement` apply profile hair, vest, pants, and skin ranges from
  `BINARYDATA/JA2PAL.DAT`.
- `Tactical/LogicalBodyTypes/Layers.cpp` builds a direction-specific,
  z-sorted layer graph.
- `Tactical/Soldier Control.cpp::ConvertAniCodeToAniFrame` rotates the world
  direction with `gOneCDirection`, adapts 1/2/3/4-direction surfaces, and computes
  `frame + framesPerDirection * spriteDirection`.
- `TileEngine/renderworld.cpp` iterates the selected layer graph and blits every
  selected STI at the same origin, leaving each ETRLE object's offsets to place
  the pixels.

Two source quirks are intentionally visible:

1. Current `Layers.cpp` reads directional z values from `zindex_*` attributes
   and ignores numeric element text. The stock `Layers.xml` uses numeric text,
   so its graph order is declaration order at z=0.
2. The source implementation's `gt`/`lt` sign comparison is inverted relative
   to the XML spelling. LOBOT Lab mirrors the source behavior.

## Rust modules

- `vfs.rs` — VFS-config discovery and ordered, case-insensitive directory/SLF
  resolution.
- `xml.rs` — external `SYSTEM` entity expansion through the active VFS.
- `loader.rs` — XML/table parsing, cross-reference validation, workspace model.
- `animation.rs` — source-derived engine state catalog and hand-item variant
  resolution.
- `filter.rs` — LOBOT filter evaluation against a test soldier.
- `sti.rs` — self-contained indexed STCI/ETRLE decoder and PNG encoder.
- `render.rs` — shared layer selection, direction/frame mapping, camouflage and
  palette application, alpha compositing, resolution trace, and completeness
  audit.
- `model.rs` — internal domain objects and serialized Tauri DTOs.

The STI decoder is kept self-contained so this project does not depend on a
separate source checkout when cloned or packaged.

## Generated source catalogs

`animation-catalog.json` and `physical-surface-catalog.json` are checked in so
normal builds never need a JA2 source checkout. Maintainers can regenerate them
with:

```sh
npm run catalog -- /path/to/ja2-v1.13-source
```

The extractor also accepts the source location through `JA2_SOURCE_ROOT`.
