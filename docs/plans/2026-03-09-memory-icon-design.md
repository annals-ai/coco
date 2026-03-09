# Coco Memory Reduction Design

**Date:** 2026-03-09

## Goal

Reduce Coco's steady-state memory usage without making result-row icons look blurry or removing clipboard image history entirely.

## Root Cause Summary

- Coco eagerly loads installed app icons during startup.
- The `.icns` fast path currently selects the largest available icon layer, which can decode 512px or 1024px RGBA bitmaps for many apps.
- Visible search results only need small icons, but the process keeps much larger decoded images resident.
- Clipboard image history is bounded by entry count, but not by image count or byte budget, so large images can still push memory up.

## Approved Approach

### 1. Decode icons to a target runtime size

- Keep source `.icns` files unchanged.
- Change runtime decoding so `.icns` uses a target-sized representation near `128px`, matching the existing `NSWorkspace` path.
- Downsample oversized icon layers before turning them into `iced::widget::image::Handle`.

Why this keeps icons sharp:
- The UI displays icons at about `38px`.
- A `128px` decoded source still leaves healthy Retina headroom, so the UI should stay crisp.

### 2. Switch installed-app icons to lazy loading

- Stop loading icons for every installed app during startup and config reload.
- Build the installed-app search index with metadata only.
- Load icons only for the currently visible app results, cache them, and reuse them across searches and zero-query state.
- Keep built-in Coco command icons and user-configured explicit icons unchanged.

### 3. Add clipboard image memory limits

- Keep text history behavior unchanged.
- Add a separate limit for clipboard image count and total image bytes held in memory.
- Trim the oldest non-pinned images first when the image budget is exceeded.

## Expected Outcome

- Much lower startup and idle memory use.
- No visible icon quality regression in normal result rows.
- Clipboard image history remains useful but cannot grow into a large memory sink.
