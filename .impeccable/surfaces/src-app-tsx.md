---
version: 1
slug: "src-app-tsx"
primary_target: "src/App.tsx"
related_targets: ["src/components/Sidebar.tsx","src/components/LibraryHealth.tsx","src/components/ProjectWorkspaceBar.tsx"]
---

# Lootbox workspace

- Scope: the main desktop asset workspace in `src/App.tsx`; mode: Operate.
- Audience: experienced indie game developers and technical artists, weighted toward high-frequency personal use and repeated Godot workflows.
- Job: browse and prepare a large local asset library while working either in Library mode or inside one explicit active Godot project.
- Primary task: establish project context once, then search, filter, select, export, remove project copies, and inspect project/library drift without choosing a destination per action.
- Proof and state: library-health counts, tracked export status, source/project drift, export history, recoverable removals, saved views, and visible active-project destination.
- Constraints: imported sources remain untouched; project edits are never overwritten; destructive Lootbox actions are unavailable while a project is active; dense expert operation outranks in-app teaching.
- Direction: extend The Quiet Toolbench. The persistent project strip and Library Health ledger make context and drift visible without adding ornamental chrome.
- Memorable moment: switching workspace context changes every project-sensitive action at once, and the destination remains visible above the asset browser.
- Unresolved decisions: none.
