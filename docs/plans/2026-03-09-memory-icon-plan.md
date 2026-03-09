# Coco Memory Fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Reduce Coco's steady-state memory usage by decoding smaller icons at runtime, lazily loading only visible app icons, and bounding clipboard image memory.

**Architecture:** Installed apps stay in the search index without eagerly loaded icon handles. The main window keeps a small in-memory icon cache keyed by bundle path, then requests icon loads only for visible results. Clipboard history keeps its text-first behavior while applying separate image count and byte budgets.

**Tech Stack:** Rust, iced, tokio, macOS AppKit/IconServices, arboard

---

### Task 1: Write the supporting docs

**Files:**
- Create: `docs/plans/2026-03-09-memory-icon-design.md`
- Create: `docs/plans/2026-03-09-memory-icon-plan.md`

**Step 1: Save the approved design**

- Capture the approved runtime-sizing, lazy-loading, and clipboard-budget approach.

**Step 2: Save the implementation plan**

- Record exact code areas, validation points, and rollout order.

### Task 2: Limit `.icns` runtime decode size

**Files:**
- Modify: `src/utils.rs`

**Step 1: Add target-aware `.icns` icon selection**

- Pick the smallest icon representation that satisfies the target size, or the largest below target.

**Step 2: Downsample oversized `.icns` images**

- Convert oversized icon layers to the target size before constructing iced image handles.

**Step 3: Add a reusable bundle icon loader**

- Centralize app-bundle icon loading behind a utility that tries `.icns` first and falls back to `NSWorkspace`.

### Task 3: Replace eager icon preloading with lazy visible-result loading

**Files:**
- Modify: `src/app.rs`
- Modify: `src/app/tile.rs`
- Modify: `src/app/tile/elm.rs`
- Modify: `src/app/tile/update.rs`
- Modify: `src/platform/macos/discovery.rs`

**Step 1: Stop eager installed-app icon loading**

- Build installed-app options with `icons: None` during startup and config reload.

**Step 2: Add an icon cache and in-flight tracking to `Tile`**

- Cache handles by bundle path and avoid duplicate concurrent icon loads.

**Step 3: Add message flow for lazy icon loading**

- Request icon loads for the currently visible main results.
- Merge completed icons back into current results and the zero-query cache.

**Step 4: Keep zero-query behavior intact**

- Continue showing running/recent apps, but resolve icons from the lazy cache instead of the preloaded app index.

### Task 4: Add clipboard image memory bounds

**Files:**
- Modify: `src/clipboard_store.rs`

**Step 1: Add image-count and image-byte budgets**

- Bound how many images and how many image bytes the in-memory store keeps.

**Step 2: Trim the oldest non-pinned images first**

- Preserve pinned entries when possible.

**Step 3: Add unit coverage for trimming behavior**

- Verify image budgets evict old image entries without affecting text entries.

### Task 5: Verify and ship locally

**Files:**
- Modify: `task_plan.md`
- Modify: `findings.md`
- Modify: `progress.md`

**Step 1: Format and test**

Run: `cargo fmt`

Run: `cargo test`

Expected: tests pass

**Step 2: Build and install**

Run: `bash /Users/kcsx/.claude/skills/coco-deploy/scripts/deploy.sh`

Expected: release build completes, app signs successfully, installs to `/Applications/Coco.app`, and launches
