---
version: alpha
name: Lootbox
description: A quiet, compact desktop workspace for finding and preparing local game assets.
colors:
  catalog-gold: "#c99a45"
  catalog-ink: "#18130b"
  archive-black: "#111214"
  paper-white: "#e8e9ec"
  card-charcoal: "#1c1e22"
  popover-charcoal: "#22252a"
  soft-white: "#e0e1e4"
  muted-charcoal: "#202227"
  quiet-slate: "#9297a1"
  active-slate: "#292c32"
  error-coral: "#d96a64"
  divider-steel: "#30343b"
  input-steel: "#383d45"
  sidebar-charcoal: "#17181b"
  sidebar-ink: "#dddfe3"
  sidebar-active: "#25272c"
  bright-ink: "#f0f0f2"
typography:
  dialog-title:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "1rem"
    fontWeight: 500
    lineHeight: 1
    fontFeature: '"ss01", "cv02", "cv03", "cv04"'
  workspace-title:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 600
    lineHeight: "1.25rem"
    letterSpacing: "-0.01em"
    fontFeature: '"ss01", "cv02", "cv03", "cv04"'
  control:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 500
    lineHeight: "1rem"
    fontFeature: '"ss01", "cv02", "cv03", "cv04"'
  metadata:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "0.6875rem"
    fontWeight: 400
    lineHeight: "1rem"
    fontFeature: '"ss01", "cv02", "cv03", "cv04"'
  technical-data:
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace"
    fontSize: "0.6875rem"
    fontWeight: 400
    lineHeight: "1rem"
rounded:
  micro: "2px"
  checkbox: "4px"
  sm: "5.36px"
  md: "8px"
  lg: "10.64px"
  xl: "13.28px"
  full: "9999px"
spacing:
  hairline: "2px"
  micro: "4px"
  xs: "6px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "20px"
components:
  button-primary:
    backgroundColor: "{colors.catalog-gold}"
    textColor: "{colors.catalog-ink}"
    typography: "{typography.control}"
    rounded: "{rounded.lg}"
    padding: "0 10px"
    height: "32px"
  button-outline:
    backgroundColor: "{colors.archive-black}"
    textColor: "{colors.paper-white}"
    typography: "{typography.control}"
    rounded: "{rounded.lg}"
    padding: "0 10px"
    height: "32px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.quiet-slate}"
    typography: "{typography.control}"
    rounded: "{rounded.md}"
    padding: "0 10px"
    height: "32px"
  input-search:
    backgroundColor: "{colors.muted-charcoal}"
    textColor: "{colors.paper-white}"
    typography: "{typography.control}"
    rounded: "{rounded.md}"
    padding: "0 32px 0 36px"
    height: "36px"
  nav-item:
    backgroundColor: "transparent"
    textColor: "{colors.quiet-slate}"
    typography: "{typography.control}"
    rounded: "{rounded.md}"
    padding: "0 10px"
    height: "32px"
  dialog:
    backgroundColor: "{colors.popover-charcoal}"
    textColor: "{colors.paper-white}"
    rounded: "{rounded.xl}"
    padding: "16px"
---

# Design System: Lootbox

## Overview

**Creative North Star: "The Quiet Toolbench"**

Lootbox is a dense desktop instrument that stays calm while the user handles a large and potentially messy asset library. Its interface feels like a well-kept technical workbench: dark, durable surfaces; one warm catalog accent; compact controls; and enough structure to make every action easy to locate without competing with the assets themselves.

The system is quiet, precise, and grounded. It favors information density, stable geometry, direct language, and low-amplitude feedback over decoration. It explicitly rejects gamer neon, glossy spectacle, ornamental gradients, and exaggerated “loot” theming. Tactility belongs to interaction states—hover, focus, press, selection—not to resting surfaces.

**Key Characteristics:**

- Compact desktop density with an 11–14px working type range.
- Charcoal tonal layers separated by fine steel borders.
- Catalog Gold reserved for primary action, focus, progress, and selection.
- Assets and previews carry the visual interest; application chrome recedes.
- Motion is brief, functional, and removed when reduced motion is requested.

## Colors

The Archive palette is a near-neutral charcoal workspace with a single muted amber signal and a coral reserved for destructive or failed states.

### Primary

- **Catalog Gold** (`#c99a45`): the sole brand and interaction accent. Use it for primary actions, selected controls, progress, focus rings, resize affordances, and small counts that need attention.
- **Catalog Ink** (`#18130b`): dark text and icon color placed on Catalog Gold.

### Neutral

- **Archive Black** (`#111214`): the main application background and the deepest resting surface.
- **Card Charcoal** (`#1c1e22`): contained recovery and card surfaces when a distinct resting panel is needed.
- **Popover Charcoal** (`#22252a`): menus, selects, dialogs, and the secondary-control surface.
- **Muted Charcoal** (`#202227`): quiet fills, skeletons, and low-emphasis hover or preview areas.
- **Active Slate** (`#292c32`): selected, focused, or hovered neutral controls.
- **Shelf Slate** (`#30343b`): borders and dividers that structure the workspace without becoming outlines around everything.
- **Input Steel** (`#383d45`): stronger field borders and disabled-field fills.
- **Paper White** (`#e8e9ec`): primary text and high-emphasis icons.
- **Quiet Slate** (`#9297a1`): descriptions, labels, metadata, and de-emphasized icons.
- **Sidebar Charcoal** (`#17181b`): the library navigation rail, separated tonally from the central workspace.

### Tertiary

- **Error Coral** (`#d96a64`): destructive actions, missing-file warnings, invalid fields, and visible failures only.

### Named Rules

**The One Warm Signal Rule.** Catalog Gold is the only positive accent. Do not introduce competing blue, purple, green, or neon interaction colors.

**The Asset Owns the Color Rule.** Application chrome remains neutral so thumbnails, textures, video, and models provide the broad color spectrum.

## Typography

**Display Font:** Geist Variable (with `sans-serif` fallback)
**Body Font:** Geist Variable (with `sans-serif` fallback)
**Label/Mono Font:** the platform UI monospace stack for counts, formats, dimensions, paths, and technical values

**Character:** Geist keeps the workspace contemporary and neutral without making it anonymous. Weight and small changes in scale establish hierarchy; technical values switch to monospace so dense metadata remains easy to scan.

### Hierarchy

- **Dialog title** (500, 16px, 1.0): the largest routine type, reserved for modal task boundaries.
- **Workspace title** (600, 14px, 20px, `-0.01em`): product name and current library section headings.
- **Control / body** (400–500, 12px, 16px): buttons, navigation, empty states, field content, and compact instructions.
- **Metadata** (400–500, 11px): section labels, supporting descriptions, paths, counts, and asset attributes.
- **Technical data** (400, 11px, monospace): file formats, byte sizes, resolution, time, numeric counts, and diagnostic details.

### Named Rules

**The Working Scale Rule.** Stay inside the implemented 11–16px desktop scale unless a genuinely new reading surface requires otherwise; hierarchy comes from structure and weight before size.

## Layout

The application is an Operate-mode, three-pane desktop workspace: a resizable library sidebar, a flexible asset browser, and a resizable detail inspector. Four-pixel resize gutters separate the columns. The default sidebar is 208–220px and the detail inspector is 320–340px; both retain constrained resize ranges while the center remains the primary working region. The packaged window starts at 1320×820 and does not support a width below 900px.

The central browser uses a 56px search/action header followed by a 48px context/action bar. Grid cards target roughly 140px columns with 12px gaps and 16px edge padding; list rows are 48px tall and align preview, name, location, and file facts in stable columns. Detail content uses 16px horizontal padding, 20px vertical section spacing, and compact 6–8px internal gaps.

At narrower desktop widths, side panels clamp down, button labels hide at 1150px, and action controls collapse to icons while preserving accessible names and tooltips. This is desktop adaptation, not a mobile reflow. Resizable separators remain keyboard-operable.

## Elevation & Depth

The system is structurally layered. Resting workspace surfaces are flat and separated by tonal shifts, one-pixel borders, sticky headers, and occasional translucent backdrops. Shadows are reserved for floating UI—menus, selects, dialogs, error recovery cards, and selected badges—where they communicate an actual change in z-position. They are not decoration for ordinary cards.

### Shadow Vocabulary

- **Floating menu** (`box-shadow: 0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)`): dropdown, context-menu, and select popovers.
- **Raised dialog** (`box-shadow: 0 20px 25px -5px rgb(0 0 0 / 0.1), 0 8px 10px -6px rgb(0 0 0 / 0.1)`): recovery or interruption surfaces that must sit above the workspace.
- **Selected badge** (`box-shadow: 0 1px 2px rgb(0 0 0 / 0.05)`): small circular selection confirmation only.

### Named Rules

**The Structural Layer Rule.** Borders and tone create the resting hierarchy; a shadow means the element is floating or responding to interaction.

## Shapes

The form language is compact and restrained. Most working controls and panels use approximately 5–8px corners; default Base Nova primitives can reach about 11px, and modal dialogs use about 13px. Four-pixel corners appear on the smallest controls, resource tiles, checkboxes, and dense inspector elements. Pills and circles are reserved for selection badges, duplicate counts, audio transport, progress tracks, and other genuinely radial or status-shaped elements.

Borders are one pixel and low-contrast. Preview frames clip media cleanly, checkerboards indicate transparency, and asset thumbnails use a consistent 4:3 frame in grid view. Do not soften the whole product with oversized radii.

## Components

Components are compact and quiet at rest, with restrained tactile feedback for hover, focus, selection, and pressing.

### Buttons

- **Shape:** 8–11px for standard controls, 5px for dense inspector controls, and full circles only for icon transport or status actions.
- **Primary:** Catalog Gold with Catalog Ink, generally 28–32px high and 10px horizontal padding.
- **Hover / Focus:** reduce or mix the fill rather than adding glow; use a three-pixel translucent Catalog Gold focus ring and a one-pixel pressed translation.
- **Outline:** Archive Black or a translucent Input Steel fill with a Shelf Slate/Input Steel border; hover toward Muted Charcoal.
- **Ghost:** transparent at rest, then Muted Charcoal on hover. Use for reversible, contextual, and toolbar actions.
- **Destructive:** Error Coral text on a low-opacity coral fill. Never borrow Catalog Gold for destructive meaning.

### Chips

- **Style:** 24–28px high, 5px corners, 11px type, and tight 6–8px horizontal padding.
- **State:** neutral outline when available, Active Slate when applied, and Catalog Gold only for the single strongest selected signal.

### Cards / Containers

- **Corner Style:** 5–8px on asset and resource containers; 13px on dialogs.
- **Background:** the card itself often remains transparent while its preview frame uses Muted Charcoal over Archive Black.
- **Shadow Strategy:** no shadow at rest; use borders, checkerboards, and tonal contrast.
- **Border:** Shelf Slate by default, Catalog Gold at selected focus, and a lighter neutral on hover.
- **Internal Padding:** 8–16px depending on information density; asset metadata remains close to its preview.

### Inputs / Fields

- **Style:** 28–36px high with an Input Steel border, transparent or very low-opacity Muted Charcoal fill, 8px default corners, and 10px horizontal padding.
- **Focus:** Catalog Gold border plus a three-pixel ring at 50% alpha.
- **Error / Disabled:** Error Coral border/ring for invalid data; disabled controls keep their layout and drop to 50% opacity.

### Navigation

Sidebar items are 32px high, use 12px Geist text and 11px monospace counts, and sit on Sidebar Charcoal. They are transparent with Quiet Slate text at rest, use Muted Charcoal for hover, and switch to Sidebar Active with brighter text for the current location. Icons remain 14px and labels truncate rather than wrapping.

### Asset Card

The asset card is the signature recurring component. In grid view it uses a 4:3 preview frame, 12px asset name, and an 11px monospace metadata line. In list view it becomes a 48px row with a 34px preview and stable columns. Selection is communicated through a Catalog Gold border, a faint gold surface tint, and a small gold confirmation badge—never a large decorative treatment.

### Floating Surfaces

Menus, selects, tooltips, and dialogs use Popover Charcoal, a faint light ring, short 100ms fade/zoom transitions, and restrained shadows. Tooltips invert to Paper White on Archive Black and open after a deliberate delay so dense icon controls remain calm.

### Quiet Acknowledgment

Delight is reserved for first use, truthful waiting, and meaningful completion. The empty archive, import stages, and completed Godot export use compact catalog geometry, Catalog Gold, and one 180–240ms settling sequence. The final state is visible without motion, reduced-motion users receive the same information immediately, and routine browsing remains still.

**The Earned Response Rule.** Never celebrate ordinary clicks or saves. Acknowledgment must confirm real progress, never delay input, loop decoratively, add sound, or compete with asset previews.

## Do's and Don'ts

### Do:

- **Do** keep application chrome near-neutral and let asset previews supply the broad color range.
- **Do** use Catalog Gold for the primary action, focus, selection, and progress signals already established by the product.
- **Do** preserve compact 11–14px information density, stable columns, truncation, and monospace technical data.
- **Do** express interaction through subtle tonal hover, visible focus, selected borders, and the existing one-pixel press movement.
- **Do** use borders and tonal layers before reaching for shadows.
- **Do** respect reduced-motion settings and retain text alternatives for icon-only controls.

### Don't:

- **Don't** introduce gamer neon, glossy gradients, ornamental loot imagery, or competing positive accent colors.
- **Don't** place floating shadows on ordinary resting cards or panels.
- **Don't** enlarge corners into soft pill-shaped containers unless the component is genuinely radial or status-like.
- **Don't** inflate headings or spacing until the workspace stops feeling like a compact desktop tool.
- **Don't** use color alone to communicate selection, missing files, errors, or destructive intent.
- **Don't** let application decoration compete with thumbnails, models, textures, waveforms, or video.
