# Lootbox

Lootbox is a quiet, local-first desktop browser for game assets. Import asset-pack folders, search their contents, preview common formats, and organize useful files without moving or modifying the originals.

## Current release

Version `0.4.0` includes:

- Multi-folder import with a cancellable, serialized queue
- Identity-preserving rescans that retain missing files, tags, and collections
- SQLite-backed metadata with FTS5 prefix search
- Transactional schema migrations, rotating backups, and backup export/restore
- Grid and compact list views
- Reversible name, date, size, and type sorting
- SHA-256 content identities with cross-pack duplicate discovery
- Godot project registration and multi-asset export to `res://assets/lootbox`
- Dependency-aware Godot exports that include grouped texture maps and model resources
- Idempotent project updates and a generated export manifest
- A redesigned adaptive workspace that gives browsing the full window until details are needed
- Visible bulk-selection actions, removable filter chips, and draft-and-apply filtering
- Richer grid and list cards with format, size, map, resolution, and duplicate context
- Purpose-built loading, empty, progress, success, error, and recovery states
- Themed confirmations, keyboard guidance, shortcut reference, and startup error recovery
- Image thumbnails cached outside the source pack
- Image, audio waveform, video, and GLB/glTF previews
- Broad format classification with safe fallback cards
- Tags and collections
- Bulk tags, collections, classification corrections, and custom grouping
- Filters for format, map role, tags, resolution, confidence, and missing files
- Versioned thumbnail cache cleanup, limits, and regeneration
- Rotating diagnostics with visible model-preview errors
- Open, reveal, copy-path, and external drag data
- Pack forgetting and collection deletion without deleting source assets

Interactive 3D previews currently support GLB and glTF. FBX, OBJ, Blender, specialized texture formats, and other recognized files are indexed and searchable but use a format card until a previewer is added.

## Run it

You need Node.js, Rust, and the [Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system.

```sh
npm install
npm run tauri dev
```

Lootbox applies WebKitGTK's DMABUF compatibility fallback on Linux to avoid a known startup crash on some NVIDIA and hybrid-GPU Wayland systems. To explicitly retry the native DMABUF renderer after a driver or WebKit update, launch with `WEBKIT_DISABLE_DMABUF_RENDERER=0`.

## Verify and package

```sh
npm run check
npm run tauri build
```

On a rolling-release Linux distribution whose system libraries use newer ELF relocation sections, use `npm run bundle:linux`; it disables `linuxdeploy`'s outdated stripping pass while still producing the Debian, RPM, and AppImage bundles.

The production frontend is built into `dist/`; native executables and installers are written below `src-tauri/target/`.

## Storage and privacy

Lootbox does not upload assets or modify imported folders. It stores a local SQLite index, rotating metadata backups, diagnostics, and generated thumbnails in the operating system's application-data directory under the identifier `com.lootbox.desktop`. Forgetting a pack removes only its Lootbox index entries. Files missing during a rescan remain recoverable in the Missing items view until explicitly purged. “Add to Godot” is the explicit exception: it copies the selected assets and their known dependencies into the chosen project's `assets/lootbox` folder and writes `lootbox-manifest.json` there.

## Project structure

- `src/` — React 19 interface using Tailwind CSS v4 and shadcn/Base UI
- `src/components/ui/` — locally owned shadcn components backed by Base UI primitives
- `src-tauri/src/lib.rs` — scanner, SQLite schema, search, and native commands
- `src-tauri/tauri.conf.json` — desktop and asset-protocol configuration
