# Story 4.16: Walking's cost

**Status:** Review (code done; AC 6 measured, research notes E86: the walking run sits on the 1 % gate without passing it, so walking stays off by default)

## Story

As the author who asked to keep walking, with a way to turn it off,
I want a walk to cost a fraction of what it cost in the release I measured,
so that walking can be on by default within FR-9's budget of 1 % of the machine.

E82 measured, on the owner's 3440 x 1440 240 Hz screen, walking's own cost at about 1.4 % of the machine (the replay with walking on 1.95 % mean, off 0.58 %, one crossing walk every 10 s), so D-480's fallback turned walking off by default (`50b4200`). The owner wants walking on: this story finds where a walk frame's cost goes and removes it at its cause.

## Acceptance Criteria

1. The cause, measured (FR-9, AD-71): a walk frame is profiled in Chromium with real frames (no fake clock) on the eight-project fixture at 3440 x 1440 and on `scene-b` at 3440 x 1440 and 1920 x 1080, at 60 Hz and with the frame rate uncapped as a stand-in for 240 Hz; the profile splits the main thread's frame into style, layout, pre-paint, paint, layerize and script, and checks what else a walk writes (the halos, the overlay, the targets, `data-moving`, the depth sort's node moves). Proof: Debug Log, with the figures before and after.
2. One band's paint per walk frame (D-550): layer 3 is split into `DEPTH_BANDS` consecutive depth bands, one `<svg>` each, the statics shared evenly and each figure in the band of the static it stands before, so the paint order is the one-root order; a walk frame writes only inside the walker or moves figure nodes among their neighbours, in one band, two when the walker crosses from one to the next. Proof: Vitest `objects.test.ts > split into depth bands, the layer paints in the order of one root` (every fixture board); Playwright `walk.spec.ts > a walk writes only the walker, in one depth band at a time`.
3. A frame floor that keeps walks smooth (D-551, AD-70, D-501): the mover draws at most one frame per `MIN_FRAME_MS` (13 ms) of refresh-aligned frame start time, so a 240 Hz screen draws 60 frames a second, a 144 Hz one 72, and a 60 or 75 Hz one every refresh; the step per drawn frame stays under 3 logical px. Proof: Vitest `mover.test.ts > a fast display draws one frame per MIN_FRAME_MS, a 60 Hz one every vsync` (240, 144, 75 and 60 Hz), `> speed never exceeds WALK.vmax` (a skipped refresh moves nothing, the next advances by the whole time); Playwright `walk.spec.ts > a walking figure stays one stable node` (under 3 px per frame).
4. Nothing redrawn at rest (D-552): the mover hands a figure at rest the same frame object until a layout or a settle changes it, and the object layer skips a frame object it has drawn. Proof: Vitest `mover.test.ts > an object at rest is handed out again unchanged, a walking one anew`.
5. Every scene test stays green: the scene, walking, motion, halo, modes and alert specs, the Vitest scene modules, and story 4.12's sub-agents, whose frames take the same path. Proof: the gate's Vitest and Playwright `--repeat-each=3` runs.
6. The walking runs measured again (AD-71 as amended, D-515, D-553): E82's runs 4 (replay, walking on) and 5 (replay, walking off), by E82's method, on the owner's screen, the session unlocked if possible, with the release window of the commit that ships this story. Walking turns back on by default (`DEFAULT_WALKING` `true`, `modes.spec.ts > walking is off by default` turned around, README and CHANGELOG) only if run 4 is under 1 % of the machine; the per-core figure is recorded as information. Proof: E82's update or the entry that records the runs; the lead schedules the quiet window (the machine is shared while this story is built).
7. Documentation: this story, D-550 to D-553, `CHANGELOG.md` `[Unreleased]`, the README's walking paragraph, `sprint-status.yaml`, the Epic 4 report. Proof: review.

## Tasks

- [x] Profile a walk frame with real frames, before (AC 1)
- [x] Depth bands in `objects.ts` and `SceneObjects.svelte`; the order test (AC 2)
- [x] The frame floor in `mover.ts`, on the frame's start time (AC 3)
- [x] Resting frames handed out again and skipped (AC 4)
- [x] Guards: the band locality case, the frame floor cases (AC 2, 3)
- [x] Profile after; the scene's tests green (AC 1, 5)
- [x] E82's runs 4 and 5 again on the owner's screen; the default decided by run 4 (AC 6)
- [x] Documentation (AC 7)

## Dev Notes

- Requirements: FR-9 (under 1 % of the machine, D-515). Decisions: AD-70, AD-71, AD-171, AD-172. Epic decisions: D-480, D-501, D-502, D-515, D-550 to D-553.
- Headless Chromium draws at 60 Hz and rasters in software: its milliseconds are not the WebView2's on the owner's machine, only their ratios carry over. `--disable-frame-rate-limit --disable-gpu-vsync` gives about 200 frames a second, the stand-in for the 240 Hz screen. The CDP LayerTree domain is left off: turned on, it tripled paint.
- The guards count, they do not time: durations depend on the machine, the writes and the frames drawn per refresh do not.
- The whole-machine CPU measurement is not run while the story is built: the machine is shared. AC 6 is its own step.

## Testing

- Vitest: `objects.test.ts`, `mover.test.ts`.
- Playwright: `walk.spec.ts`, and every scene spec unchanged.
- Workstation: E82's runs 4 and 5 (AC 6).

## Dev Agent Record

### Debug Log

- Method (AC 1): a throwaway spec, never committed, with real `requestAnimationFrame`, counting CDP `Performance.getMetrics` deltas per frame, reading a Chromium trace's main-thread events per frame, and logging DOM mutations, over one Working session walking to the decision office and back: two walks on `scene-eight` (19 figures, 5,611 nodes) at 3440 x 1440, two on `scene-b` (10 figures) at 3440 x 1440 and 1920 x 1080. Run in Playwright's Chromium on the workstation by a sub-agent of this story, in its own worktree; the figures below are that run's.
- Before, main thread per frame at 60 Hz:

  | | scene-eight, 3440 | scene-b, 1920 |
  |---|---|---|
  | Whole frame | 3.10 ms | 2.23 ms |
  | Paint | 1.39 to 1.46 ms | 0.96 ms |
  | Layerize | 0.87 ms | 0.61 ms |
  | Layout | 0.28 ms | 0.17 ms |
  | rAF script | 0.22 ms | 0.22 ms |
  | Pre-paint | 0.19 ms | 0.13 ms |
  | Style | 0.09 ms | 0.08 ms |
  | Raster, off the main thread | 0.40 ms | 0.35 ms |

  An idle frame costs about 0.22 ms. A glide costs the same per frame as a walk, but lasts 320 ms where a crossing walk lasts about 9 s.
- The cause: paint and layerize, about 70 % of the frame. Moving one figure dirtied the single objects `<svg>`, so Chromium repainted the whole layer every frame, and paint and layerize grow with the layer, not with the walker; at 240 Hz that work runs four times as often as at 60 Hz.
- Ruled out, each measured: the walk cycle's limbs (about 0.4 ms of the frame; the translate alone repaints the layer); the depth sort (a node moved about 0.25 times per frame); `data-moving` (no CSS reads it); the halos, the overlay and the targets (nothing written during a walk; un-compositing the halos changed nothing); `will-change: transform` on the walking figure (layerize doubled, to 1.75 ms) or on the whole layer (no change); each band composited (layerize 0.79 to 0.60 ms, not worth the layers' memory, D-550).
- After, at 60 Hz: `scene-eight` 2.32 ms per frame (paint 0.37 to 0.43 ms, SVG layout 0.07 to 0.08 ms, layerize about 0.85 ms, unchanged), `scene-b` 1.64 ms at 3440 and 1.65 ms at 1920. Uncapped, main-thread time per second of walking: `scene-eight` 455 to 109 ms (178 to 48.5 layouts a second), `scene-b` 348 to 77 ms at 3440 and 415 to 80 ms at 1920, a cut of 76 to 81 %; the walks took the same wall time. 24 bands gave 0.26 ms of paint against 0.37 ms for 8 (D-550). These figures are the prototype's, whose floor read `performance.now()`; the shipped floor reads the frame's start time (below), which draws the same frames on these refresh rates.
- The paint area could not be read: the trace's `Paint` clip is the whole layer, not the dirty rectangle, so paint's duration is the signal.
- The frame floor first compared `performance.now()` read in the callback; a 75 Hz screen's 13.3 ms frames, read with the callback's jitter, would have dropped frames at random. It reads the frame's own start time, which a browser aligns on the refresh (D-551); a loop that starts again finds the last drawn frame long past, so its first frame draws.
- With the floor, a sub-agent's hold (D-548) ticks at the display's rate but draws once per 13 ms, like a walk.
- Not measured: an empty refresh at 240 Hz still runs the callback, which returns at once; its cost under a real 240 Hz refresh is a guess (about the idle frame's 0.2 ms at most) until AC 6.

### Completion Notes

- AC 1: Debug Log.
- AC 2: `objects.test.ts > split into depth bands, the layer paints in the order of one root`; `walk.spec.ts > a walk writes only the walker, in one depth band at a time` (it also holds each band to an even share of the statics, so the layer cannot quietly go back to one root).
- AC 3: `mover.test.ts > a fast display draws one frame per MIN_FRAME_MS, a 60 Hz one every vsync`, `> speed never exceeds WALK.vmax`; `walk.spec.ts > a walking figure stays one stable node`.
- AC 4: `mover.test.ts > an object at rest is handed out again unchanged, a walking one anew`.
- AC 5: the gate's figures (the Epic 4 report).
- AC 6: **measured, gate not passed** (2026-10-29, research notes E86), the release window of `d33d0c2` (this story's code), session locked as in E82 (about 170 frames per second): run 4, replay with walking on, 0.9985 % of the machine (24.0 % of one core) and 1.007 % (24.2 %) when run again, both over 1 % from minute 2 on; run 5, walking off, 0.51 % (12.1 %). Walking's own cost fell from about 1.4 % to 0.5 % of the machine. Run 4 is not under 1 %, so D-553 does not apply: `DEFAULT_WALKING` stays `false`.

## File List

- `app/src/lib/scene/objects.ts`, `mover.ts`, `geometry.ts` (`DEPTH_BANDS`, where the e2e specs can import it: their TypeScript project has no DOM iterables for `objects.ts`)
- `app/src/components/scene/SceneObjects.svelte`
- `app/src/lib/scene/objects.test.ts`, `mover.test.ts`
- `app/e2e/walk.spec.ts`
- `README.md`, `CHANGELOG.md`
- `docs/research-notes.md` (E86)

## Change Log

- 2026-10-29 created after E82's walking run failed and the owner asked to keep walking; implemented (D-550 to D-553); AC 6 open for the lead
- 2026-10-29 AC 6 measured (E86): run 4 at 1.00 % of the machine, not under the gate; walking stays off by default
