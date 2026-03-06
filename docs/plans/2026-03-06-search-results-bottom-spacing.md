# Search Results Bottom Spacing Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a small bottom breathing room only to the populated main search results list so the last app row does not touch the window edge.

**Architecture:** Keep the change narrowly scoped to the main search-results view. Add one shared constant for the extra bottom gap, render a spacer at the end of `search_results_view`, and include the same height in the search-results sizing formula so the native window and blur height stay in sync.

**Tech Stack:** Rust, iced, existing Coco layout constants and height calculation helpers.

---

### Task 1: Add shared layout constant

**Files:**
- Modify: `/Users/kcsx/Project/kcsx/coco/src/app.rs`

**Step 1: Write the failing test**

Define a focused unit test in the height-calculation module that expects populated search results to include the extra bottom gap.

**Step 2: Run test to verify it fails**

Run: `cargo test search_results_height_includes_bottom_spacing -- --nocapture`

**Step 3: Write minimal implementation**

Add a shared `MAIN_SEARCH_RESULTS_BOTTOM_SPACING` constant in `src/app.rs`.

**Step 4: Run test to verify it passes**

Run the same `cargo test` command and confirm it passes.

### Task 2: Render and measure bottom spacing

**Files:**
- Modify: `/Users/kcsx/Project/kcsx/coco/src/app/tile/elm.rs`
- Modify: `/Users/kcsx/Project/kcsx/coco/src/app/tile/update.rs`

**Step 1: Render spacer**

Append a fixed-height spacer to `search_results_view` after the last row.

**Step 2: Keep sizing logic aligned**

Update `search_results_scrollable_height` to add the same fixed-height spacing value.

**Step 3: Verify behavior**

Run the targeted test and a search regression test sweep.

### Task 3: Format and verify

**Files:**
- Modify: `/Users/kcsx/Project/kcsx/coco/src/app.rs`
- Modify: `/Users/kcsx/Project/kcsx/coco/src/app/tile/elm.rs`
- Modify: `/Users/kcsx/Project/kcsx/coco/src/app/tile/update.rs`

**Step 1: Format**

Run: `cargo fmt`

**Step 2: Run tests**

Run:
- `cargo test search_results_height_includes_bottom_spacing -- --nocapture`
- `cargo test search::tests:: -- --nocapture`

**Step 3: Manual verification**

Search for a term with a short result list and confirm the last row no longer sits flush against the window’s bottom edge.
