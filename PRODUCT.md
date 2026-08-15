# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Lootbox is primarily for experienced indie game developers and technical artists who manage growing local libraries of downloaded game-asset packs. It is first designed around its maker's own high-frequency workflow and other power users working repeatedly in Godot projects.

Users need to find, inspect, organize, and reuse assets across scattered folders without disturbing the source packs they downloaded or purchased.

In-product teaching is intentionally secondary. Concise labels, visible state, and safe recovery must make operation dependable, while longer onboarding and conceptual documentation may live outside the application.

## Product Purpose

Lootbox turns local asset-pack folders into a searchable, previewable desktop library. It helps users understand what they already own, identify useful files and related resources, organize them for later use, and move a selected asset with its known dependencies into a Godot project.

Success means a user can move from an untidy collection of packs to a project-ready asset quickly, confidently, and without manually browsing or reorganizing the original folders.

## Positioning

Lootbox is a private, non-destructive desktop asset library: it combines local indexing, rich previews, classification, organization, duplicate discovery, and dependency-aware Godot export while leaving imported source folders untouched.

## Operating Context

- Users import one or more asset-pack folders from local storage.
- Lootbox indexes asset metadata into a local SQLite database and keeps generated thumbnails, rotating backups, and diagnostics in its application-data directory.
- Users browse in grid or compact-list views; search names, paths, packs, and tags; filter and sort results; inspect previews and metadata; and organize assets with tags and collections.
- Users may rescan relocated or changed packs while retaining missing-file records, tags, collections, and asset identity.
- Godot users can register projects and explicitly export selected assets and known dependencies to `res://assets/lootbox`, with a generated manifest.
- One registered Godot project can be the active workspace context. Export, project removal, status, and history actions target that project until the user returns to Library mode or activates another project.
- Common actions include opening or revealing a source file, copying its path, dragging it to another application, correcting classification, and managing duplicate or missing assets.

## Capabilities and Constraints

- Lootbox is a desktop application built with a React interface in a Tauri shell and a Rust/SQLite local backend.
- Asset data stays on the user’s machine; Lootbox does not upload imported assets.
- Importing, indexing, previewing, tagging, collecting, rescanning, and forgetting packs must not modify or delete files in imported source folders.
- Godot export is an explicit exception to the no-copy rule: it copies selected assets and known dependencies into the chosen project.
- Indexed formats include images, textures, audio, models, video, fonts, shaders, materials, archives, and safe fallback types.
- Image, audio waveform, video, GLB, and glTF previews are supported. Other recognized formats may use informational format cards until a previewer exists.
- Search and metadata are SQLite-backed, including FTS5 prefix search and SHA-256 identities for cross-pack duplicate discovery.
- The product supports bulk selection, tagging, collections, classification corrections, filtering, sorting, cache management, metadata backup and restore, and recovery-oriented missing-file handling.
- Current release and packaging evidence is Linux-oriented, while Tauri remains the desktop application boundary.
- Product claims must be supported by implemented behavior or supplied evidence; future work must not fabricate customers, testimonials, benchmarks, pricing, licensing, or deployment claims.

## Brand Commitments

- The product name is **Lootbox**.
- The durable product character is quiet, local-first, capable, and respectful of the user’s files.
- “Non-destructive” means imported source folders remain untouched unless the user initiates a separate, explicit export into a project.
- Existing application icons and packaging assets are maintained under `src-tauri/icons/` and `packaging/`.

## Evidence on Hand

- `README.md` documents the current product promise, release capabilities, storage behavior, privacy boundaries, and supported workflows.
- `src/` contains the working React interface and its automated tests.
- `src-tauri/src/lib.rs` contains the local scanner, SQLite schema and migrations, search, backup, preview support, and native commands.
- `src-tauri/tauri.conf.json` and `packaging/` contain the desktop identity and packaging metadata.
- Release bundles for versions through `0.4.0` are present under `src-tauri/target/release/bundle/`.
- No external testimonials, customer logos, usage benchmarks, press coverage, or other third-party proof are currently recorded in the repository.

## Product Principles

1. Keep the user’s source library safe: observing and organizing assets must not mutate their originals.
2. Make large, messy collections legible through fast search, useful previews, trustworthy metadata, and recoverable organization.
3. Reduce the distance from discovery to use by carrying selected assets and known dependencies cleanly into a Godot project.
4. Prefer explicit, reversible actions and visible recovery paths for imports, missing files, metadata, caches, and destructive-looking operations.
5. Keep private asset libraries local and make product claims only when the implementation or supplied evidence supports them.
6. Optimize repeated expert use: preserve working context, make bulk and keyboard actions predictable, and surface project/library drift without requiring the user to remember prior operations.

## Accessibility & Inclusion

The desktop workflow supports keyboard guidance and shortcut discovery. The interface respects reduced-motion preferences, uses accessible labels on interactive controls, and should remain operable without relying solely on pointer input or animation.
