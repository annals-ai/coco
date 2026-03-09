# Coco Async Icon Placeholder Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Keep icon loading asynchronous while preventing result rows from appearing blank or flashing when icons arrive late.

**Architecture:** Cached icon handles are reapplied synchronously during result-list rebuilds, while uncached icons continue loading in the background. A stronger placeholder badge makes pending icons visually stable until the real handle arrives.

**Tech Stack:** Rust, iced, tokio, macOS icon loading helpers

---

### Task 1: Rehydrate cached icons before rendering

**Files:**
- Modify: `src/app/tile/update.rs`

**Steps:**
- Apply cached icons right after zero-query lists are rebuilt.
- Apply cached icons right after searchable main results are rebuilt.
- Keep async loading only for genuinely uncached bundle paths.

### Task 2: Improve the pending-icon placeholder

**Files:**
- Modify: `src/app/apps.rs`

**Steps:**
- Add a reusable placeholder badge for app rows.
- Use the app's first visible character when possible.
- Keep dimensions identical to the real icon slot to avoid layout shift.

### Task 3: Verify and reinstall

**Files:**
- Modify: `task_plan.md`
- Modify: `findings.md`
- Modify: `progress.md`

**Steps:**
- Run `cargo fmt`
- Run `cargo test`
- Run `bash /Users/kcsx/.claude/skills/coco-deploy/scripts/deploy.sh`
