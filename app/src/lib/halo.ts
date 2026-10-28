// The attention halo's timing (AD-65, D-108): every halo breathes in one
// phase shared with the wall clock, for `HALO_BREATHING_MS` after its wait
// began, then rests at `--halo-rest`. The timing of a wait is computed once,
// at first sight, and kept while the key waits, so a re-render, a remount or
// a mode switch never restarts the breathing.
import type { Action } from "svelte/action";
import type { Card } from "./generated/Card";

/** One direction of the breath, `from` to `to` or back. */
export const HALO_HALF_MS = 2_000;
/** A full breath, out and back: the shared phase repeats at this period. */
export const HALO_CYCLE_MS = 2 * HALO_HALF_MS;
/** How long a new wait breathes; 0 makes every halo steady (FR-9's
 *  fallback). */
export const HALO_BREATHING_MS = 120_000;
/** The keyframes of `app.css` the action anchors. */
export const HALO_ANIMATION = "halo-breathe";

export interface HaloTiming {
  /** Wall-clock time the breathing counts from: a multiple of the cycle, so
   *  every halo shares one phase. */
  startMs: number;
  /** Halves to run from `startMs`; odd, so the last one ends at rest. */
  iterations: number;
}

/** The timing of a wait that began at `enteredAtMs`, first seen at
 *  `firstSeenMs`: breathing from the cycle boundary at or before first
 *  sight, for the smallest odd number of halves that reaches
 *  `enteredAtMs + HALO_BREATHING_MS`; none when the wait is already that
 *  old. A start later than first sight (a clock ahead of the window's)
 *  counts from first sight, so no halo breathes longer than the rule. */
export function haloTiming(enteredAtMs: number, firstSeenMs: number): HaloTiming {
  const startMs = Math.floor(firstSeenMs / HALO_CYCLE_MS) * HALO_CYCLE_MS;
  const enteredMs = Math.min(enteredAtMs, firstSeenMs);
  if (firstSeenMs - enteredMs >= HALO_BREATHING_MS) return { startMs, iterations: 0 };
  const halves = Math.ceil((enteredMs + HALO_BREATHING_MS - startMs) / HALO_HALF_MS);
  return { startMs, iterations: halves % 2 === 1 ? halves : halves + 1 };
}

/** The timings of the waits on screen, by key and by when the wait began.
 *  A plain map, not reactive state: pruning it never re-renders. */
export class HaloMemo {
  readonly #clock: () => number;
  readonly #timings = new Map<string, Map<number | null, HaloTiming>>();

  constructor(clock: () => number = () => Date.now()) {
    this.#clock = clock;
  }

  now(): number {
    return this.#clock();
  }

  /** The timing of `key` waiting since `enteredAt` (`null`: unknown, the
   *  wait counts from first sight); the clock is read on a new pair only. */
  timing(key: string, enteredAt: number | null): HaloTiming {
    let byStart = this.#timings.get(key);
    const known = byStart?.get(enteredAt);
    if (known) return known;
    const firstSeenMs = this.#clock();
    const timing = haloTiming(enteredAt ?? firstSeenMs, firstSeenMs);
    if (!byStart) {
      byStart = new Map();
      this.#timings.set(key, byStart);
    }
    byStart.set(enteredAt, timing);
    return timing;
  }

  /** Forgets every key not in `keys`: a key that waits again later is a
   *  new wait. */
  retain(keys: Iterable<string>): void {
    const waiting = new Set(keys);
    for (const key of this.#timings.keys()) if (!waiting.has(key)) this.#timings.delete(key);
  }
}

/** The one memo the board, the scene and `BoardStore` share. */
export const haloMemo = new HaloMemo();

/** What a halo element breathes for; `null` when it shows no halo. */
export interface HaloPair {
  key: string;
  /** When the wait began: `last_event_at_ms`, else `started_at_ms`, else
   *  `null` for first sight. */
  enteredAt: number | null;
}

/** When `card`'s wait began, as a halo counts it. */
export function haloEnteredAt(card: Card): number | null {
  return card.last_event_at_ms ?? card.started_at_ms;
}

const PROPERTIES = ["--halo-delay", "--halo-count", "--halo-half"] as const;

/** True for the halo animation of `node` itself or of its pseudo-elements,
 *  never of a descendant that breathes on its own. */
function isHaloOf(node: HTMLElement, animation: Animation): boolean {
  return (
    animation instanceof CSSAnimation &&
    animation.animationName === HALO_ANIMATION &&
    animation.effect instanceof KeyframeEffect &&
    animation.effect.target === node
  );
}

/** The `breathe` action over `memo`. It writes `--halo-delay`,
 *  `--halo-count` and `--halo-half` on the element, which its CSS feeds to
 *  `animation: halo-breathe ...` on itself or a pseudo-element.
 *
 *  A CSS delay counts from the moment the browser starts the animation, so
 *  the delay is `startMs` minus that moment: written at mount, written again
 *  whenever the browser starts the animation anew (a node moved by a keyed
 *  list, reduced motion turned off), and on a new pair, where the animation
 *  is sent back to its start so the new delay counts from now: a changed
 *  delay alone would count from the old start. An update carrying the same
 *  pair changes nothing. */
export function breatheWith(memo: HaloMemo): Action<HTMLElement, HaloPair | null> {
  return (node, initial) => {
    let pair = initial ?? null;

    function write(): void {
      if (!pair) {
        for (const property of PROPERTIES) node.style.removeProperty(property);
        return;
      }
      const { startMs, iterations } = memo.timing(pair.key, pair.enteredAt);
      const nowMs = memo.now();
      node.style.setProperty("--halo-delay", `${startMs - nowMs}ms`);
      node.style.setProperty("--halo-count", String(iterations));
      node.style.setProperty("--halo-half", `${HALO_HALF_MS}ms`);
    }

    function onStart(event: AnimationEvent): void {
      if (event.target === node && event.animationName === HALO_ANIMATION) write();
    }

    write();
    node.addEventListener("animationstart", onStart);
    return {
      update(next) {
        const nextPair = next ?? null;
        if (pair?.key === nextPair?.key && pair?.enteredAt === nextPair?.enteredAt) return;
        pair = nextPair;
        write();
        for (const animation of node.getAnimations({ subtree: true })) {
          if (isHaloOf(node, animation)) animation.currentTime = 0;
        }
      },
      destroy() {
        node.removeEventListener("animationstart", onStart);
      },
    };
  };
}

/** The action every halo uses (board ring, scene halo, beacon glow). */
export const breathe = breatheWith(haloMemo);
