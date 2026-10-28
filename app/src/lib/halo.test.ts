import { afterEach, describe, expect, it, vi } from "vitest";
import type { ActionReturn } from "svelte/action";
import {
  HALO_ANIMATION,
  HALO_BREATHING_MS,
  HALO_CYCLE_MS,
  HALO_HALF_MS,
  HaloMemo,
  breatheWith,
  haloTiming,
  type HaloPair,
} from "./halo";

const T0 = 1_800_000_000_123;

/** A clock the test moves by hand, counting its reads. */
function manualClock(startMs: number): { clock: ReturnType<typeof vi.fn<() => number>>; set(ms: number): void } {
  let now = startMs;
  const clock = vi.fn(() => now);
  return { clock, set: (ms) => (now = ms) };
}

type Handle = ActionReturn<HaloPair | null>;

function mount(memo: HaloMemo, pair: HaloPair | null): { node: HTMLElement; handle: Handle } {
  const node = document.createElement("article");
  const handle = breatheWith(memo)(node, pair) as Handle;
  return { node, handle };
}

function styleOf(node: HTMLElement): { delay: string; count: string; half: string } {
  return {
    delay: node.style.getPropertyValue("--halo-delay"),
    count: node.style.getPropertyValue("--halo-count"),
    half: node.style.getPropertyValue("--halo-half"),
  };
}

/** The end of the breathing a timing gives, in wall-clock time. */
function endOf(timing: { startMs: number; iterations: number }): number {
  return timing.startMs + timing.iterations * HALO_HALF_MS;
}

// jsdom has no Web Animations: just enough of it for the action's restart.
class FakeKeyframeEffect {
  constructor(readonly target: Element) {}
}
class FakeCSSAnimation {
  currentTime: number | null = 1_234;
  constructor(
    readonly animationName: string,
    readonly effect: FakeKeyframeEffect,
  ) {}
}

function withAnimations(node: HTMLElement, animations: FakeCSSAnimation[]): void {
  vi.stubGlobal("CSSAnimation", FakeCSSAnimation);
  vi.stubGlobal("KeyframeEffect", FakeKeyframeEffect);
  Object.assign(node, { getAnimations: () => animations });
}

function animationStart(target: EventTarget, animationName: string): void {
  const event = Object.assign(new Event("animationstart", { bubbles: true }), { animationName });
  target.dispatchEvent(event);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("haloTiming", () => {
  it("start is a multiple of the cycle, so every halo shares the wall-clock phase", () => {
    for (const offset of [0, 1, HALO_HALF_MS, HALO_CYCLE_MS - 1, HALO_CYCLE_MS, 7 * HALO_CYCLE_MS + 3]) {
      const firstSeen = T0 + offset;
      const { startMs } = haloTiming(firstSeen - 1_000, firstSeen);
      expect(startMs % HALO_CYCLE_MS).toBe(0);
      expect(startMs).toBeLessThanOrEqual(firstSeen);
      expect(firstSeen - startMs).toBeLessThan(HALO_CYCLE_MS);
    }
  });

  it("iterations are odd and cover the breathing time", () => {
    for (let age = 0; age < HALO_BREATHING_MS; age += 997) {
      const entered = T0 - age;
      const timing = haloTiming(entered, T0);
      expect(timing.iterations % 2).toBe(1);
      expect(endOf(timing)).toBeGreaterThanOrEqual(entered + HALO_BREATHING_MS);
      // The smallest such count: two halves fewer would stop short.
      expect(endOf({ ...timing, iterations: timing.iterations - 2 })).toBeLessThan(entered + HALO_BREATHING_MS);
    }
  });

  it("a wait older than the breathing time at first sight gets no iteration", () => {
    expect(haloTiming(T0 - HALO_BREATHING_MS, T0).iterations).toBe(0);
    expect(haloTiming(T0 - HALO_BREATHING_MS - 1, T0).iterations).toBe(0);
    expect(haloTiming(T0 - 5 * HALO_BREATHING_MS, T0).iterations).toBe(0);
    expect(haloTiming(T0 - HALO_BREATHING_MS + 1, T0).iterations).toBeGreaterThan(0);
  });

  it("a start later than first sight breathes no longer than from first sight", () => {
    const ahead = haloTiming(T0 + 10 * 60_000, T0);
    expect(ahead).toEqual(haloTiming(T0, T0));
    expect(endOf(ahead) - T0).toBeLessThan(HALO_BREATHING_MS + 2 * HALO_HALF_MS);
  });
});

describe("HaloMemo", () => {
  it("a second request returns the same timing without reading the clock", () => {
    const { clock, set } = manualClock(T0);
    const memo = new HaloMemo(clock);
    const first = memo.timing("pid:1:1", T0 - 10_000);
    expect(clock).toHaveBeenCalledOnce();
    set(T0 + 45_000);
    expect(memo.timing("pid:1:1", T0 - 10_000)).toEqual(first);
    expect(clock).toHaveBeenCalledOnce();
  });

  it("a new enteredAt for the same key starts a new timing", () => {
    const { clock, set } = manualClock(T0);
    const memo = new HaloMemo(clock);
    const first = memo.timing("pid:1:1", T0 - 10_000);
    const later = T0 + 5 * 60_000;
    set(later);
    const second = memo.timing("pid:1:1", later - 1_000);
    expect(clock).toHaveBeenCalledTimes(2);
    expect(second).toEqual(haloTiming(later - 1_000, later));
    expect(second).not.toEqual(first);
    // Two halos of one key on different starts (the board ring and the
    // alert room's beacon) each keep theirs.
    expect(memo.timing("pid:1:1", T0 - 10_000)).toEqual(first);
    expect(clock).toHaveBeenCalledTimes(2);
  });

  it("a card without times breathes from first sight", () => {
    const { clock, set } = manualClock(T0);
    const memo = new HaloMemo(clock);
    const timing = memo.timing("bg:a1", null);
    expect(timing).toEqual(haloTiming(T0, T0));
    expect(endOf(timing)).toBeGreaterThanOrEqual(T0 + HALO_BREATHING_MS);
    set(T0 + 30_000);
    expect(memo.timing("bg:a1", null)).toEqual(timing);
    expect(clock).toHaveBeenCalledOnce();
  });

  it("retain drops keys that no longer wait", () => {
    const { clock, set } = manualClock(T0);
    const memo = new HaloMemo(clock);
    const kept = memo.timing("pid:1:1", T0 - 1_000);
    memo.timing("pid:2:2", T0 - 1_000);
    memo.retain(["pid:1:1"]);
    set(T0 + 3 * 60_000);
    expect(memo.timing("pid:1:1", T0 - 1_000)).toEqual(kept);
    expect(clock).toHaveBeenCalledTimes(2);
    // The dropped key is seen anew: three minutes on, its wait is steady.
    expect(memo.timing("pid:2:2", T0 - 1_000)).toEqual(haloTiming(T0 - 1_000, T0 + 3 * 60_000));
    expect(memo.timing("pid:2:2", T0 - 1_000).iterations).toBe(0);
    expect(clock).toHaveBeenCalledTimes(3);
  });
});

describe("breathe", () => {
  it("the action writes the style once and ignores an update with the same pair", () => {
    const { clock, set } = manualClock(T0);
    const memo = new HaloMemo(clock);
    const pair = { key: "pid:1:1", enteredAt: T0 - 10_000 };
    const { node, handle } = mount(memo, pair);
    const timing = haloTiming(pair.enteredAt, T0);
    expect(styleOf(node)).toEqual({
      delay: `${timing.startMs - T0}ms`,
      count: String(timing.iterations),
      half: `${HALO_HALF_MS}ms`,
    });
    const written = styleOf(node);
    const reads = clock.mock.calls.length;
    set(T0 + 20_000);
    handle.update!({ ...pair });
    expect(styleOf(node)).toEqual(written);
    expect(clock).toHaveBeenCalledTimes(reads);
  });

  it("the action re-applies on a new pair", () => {
    const { clock, set } = manualClock(T0);
    const memo = new HaloMemo(clock);
    const { node, handle } = mount(memo, { key: "pid:1:1", enteredAt: T0 - 10 * 60_000 });
    expect(styleOf(node).count).toBe("0");
    const halo = new FakeCSSAnimation(HALO_ANIMATION, new FakeKeyframeEffect(node));
    const other = new FakeCSSAnimation("spin", new FakeKeyframeEffect(node));
    const inner = new FakeCSSAnimation(HALO_ANIMATION, new FakeKeyframeEffect(document.createElement("i")));
    withAnimations(node, [halo, other, inner]);
    const later = T0 + 90_000;
    set(later);
    handle.update!({ key: "pid:1:1", enteredAt: later - 2_000 });
    const timing = haloTiming(later - 2_000, later);
    expect(timing.iterations).toBeGreaterThan(0);
    expect(styleOf(node)).toEqual({
      delay: `${timing.startMs - later}ms`,
      count: String(timing.iterations),
      half: `${HALO_HALF_MS}ms`,
    });
    // The running animation counts its new delay from now; others are left.
    expect(halo.currentTime).toBe(0);
    expect(other.currentTime).toBe(1_234);
    expect(inner.currentTime).toBe(1_234);
  });

  it("a remount later keeps the phase and the end of the breathing", () => {
    const { clock, set } = manualClock(T0);
    const memo = new HaloMemo(clock);
    const pair = { key: "pid:1:1", enteredAt: T0 - 10_000 };
    const first = mount(memo, pair);
    first.handle.destroy!();
    const later = T0 + 37_000;
    set(later);
    const second = mount(memo, pair);
    const timing = haloTiming(pair.enteredAt, T0);
    // Same count, delay moved by the time passed: the animation, started now,
    // is where the first one would be, and ends when it would have.
    expect(styleOf(second.node).count).toBe(String(timing.iterations));
    expect(styleOf(second.node).delay).toBe(`${timing.startMs - later}ms`);
    const startOfSecond = later + parseFloat(styleOf(second.node).delay);
    expect(startOfSecond).toBe(timing.startMs);
    expect(startOfSecond % HALO_CYCLE_MS).toBe(0);
  });

  it("an animation the browser starts anew counts its delay from that moment", () => {
    const { clock, set } = manualClock(T0);
    const memo = new HaloMemo(clock);
    const pair = { key: "pid:1:1", enteredAt: T0 - 10_000 };
    const { node } = mount(memo, pair);
    const timing = haloTiming(pair.enteredAt, T0);
    const later = T0 + 50_000;
    set(later);
    // Another animation, or a halo of a descendant, leaves the delay alone.
    animationStart(node, "spin");
    const child = node.appendChild(document.createElement("span"));
    animationStart(child, HALO_ANIMATION);
    expect(styleOf(node).delay).toBe(`${timing.startMs - T0}ms`);
    // Its own halo started again (a keyed move, reduced motion turned off).
    animationStart(node, HALO_ANIMATION);
    expect(styleOf(node).delay).toBe(`${timing.startMs - later}ms`);
    expect(styleOf(node).count).toBe(String(timing.iterations));
  });

  it("a card that does not wait gets no halo, and loses it when it stops waiting", () => {
    const memo = new HaloMemo(manualClock(T0).clock);
    const { node, handle } = mount(memo, null);
    expect(styleOf(node)).toEqual({ delay: "", count: "", half: "" });
    withAnimations(node, []);
    handle.update!({ key: "pid:1:1", enteredAt: T0 });
    expect(styleOf(node).count).not.toBe("");
    handle.update!(null);
    expect(styleOf(node)).toEqual({ delay: "", count: "", half: "" });
  });
});
