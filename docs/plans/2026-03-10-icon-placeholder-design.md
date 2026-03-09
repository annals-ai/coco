# Coco Async Icon Placeholder Design

**Date:** 2026-03-10

## Goal

Keep icon loading asynchronous, but avoid rows briefly looking empty or visibly flashing when real app icons are still loading.

## Root Cause

- Search and zero-query result rows can render before cached icons are synchronously reapplied.
- When that happens, the UI falls back to a very light generic placeholder, which can feel like an empty slot.
- A moment later, the async icon load or cache hydration updates the row again, which reads like a flicker.

## Approved Approach

### 1. Rehydrate cached icons before first paint

- Whenever search results or zero-query lists are rebuilt, immediately apply any already-cached icon handles before the list is rendered.
- Async loading stays in place only for truly unseen icons.

### 2. Use a stronger placeholder while icons are pending

- Replace the weak empty-looking fallback with a stable placeholder badge.
- Use the app's first visible character when possible, inside a subtle rounded tile, so the row still feels intentional before the real icon arrives.

## Expected Outcome

- Previously seen results should show their icons immediately, without a placeholder flash.
- First-time results should still load asynchronously, but display a stable placeholder instead of looking blank.
