// The workshop limits the window sizes itself for (D-618), from the vector
// file core rebuilds from `floor::MAX_WORKERS_RANGE`, so the window cannot
// drift from it. The import attribute lets the Playwright specs load this
// module too.
import limits from "../../../tests/vectors/window_limits.json" with { type: "json" };

/** `[workshop] max_workers`' range, first and last inclusive. */
export const MAX_WORKERS_RANGE: readonly [number, number] = [limits.max_workers[0] ?? 1, limits.max_workers[1] ?? 1];

/** The characters the live-run count can need. */
export const LIVE_WIDTH_CH = String(MAX_WORKERS_RANGE[1]).length;
