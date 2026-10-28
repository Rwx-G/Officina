import { afterEach, describe, expect, it, vi } from "vitest";
import { BoardStore, COPIED_VISIBLE_MS, OUTCOME_VISIBLE_MS } from "./board.svelte";
import { startAll } from "./lifecycle";
import type { Card } from "./generated/Card";

const unlisten = vi.fn();

vi.mock("./ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./ipc")>()),
  onBoard: vi.fn(async () => unlisten),
  getBoard: vi.fn(async () => null),
  focus: vi.fn(async () => ({ outcome: "focused" })),
  attachCommand: vi.fn(async () => "claude attach s"),
}));

function card(key: string): Card {
  return {
    key,
    column: "needs_you",
    badge: null,
    kind: "interactive",
    name: "billing-api",
    project: { name: "billing-api", color: "blue" },
    session_id: "s",
    background_id: null,
    pid: 1,
    cwd: "C:/work/billing-api",
    waiting_for: "permission prompt",
    last_prompt: null,
    last_message: null,
    last_event_at_ms: 0,
    started_at_ms: 0,
    attach_available: true,
    subject: null,
  };
}

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("BoardStore timers", () => {
  it("keeps one expiry per card and its stop clears every timer", async () => {
    vi.useFakeTimers();
    const store = new BoardStore();
    const stop = await store.start();
    expect(vi.getTimerCount()).toBe(1);
    await store.focus(card("pid:1:1"));
    await vi.advanceTimersByTimeAsync(2_000);
    await store.focus(card("pid:1:1"));
    await store.focus(card("pid:2:2"));
    expect(vi.getTimerCount()).toBe(3);
    stop();
    expect(vi.getTimerCount()).toBe(0);
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("drops an outcome once it has been shown its time", async () => {
    vi.useFakeTimers();
    const store = new BoardStore();
    const stop = await store.start();
    await store.focus(card("pid:1:1"));
    expect(Object.keys(store.outcomes)).toEqual(["pid:1:1"]);
    await vi.advanceTimersByTimeAsync(OUTCOME_VISIBLE_MS + 100);
    expect(store.outcomes).toEqual({});
    stop();
  });

  it("says a copy for its time only", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("navigator", { clipboard: { writeText: vi.fn(async () => undefined) } });
    const store = new BoardStore();
    const stop = await store.start();
    await store.copyAttach(card("pid:1:1"));
    expect(store.copied).toBe("pid:1:1");
    await vi.advanceTimersByTimeAsync(COPIED_VISIBLE_MS - 1);
    expect(store.copied).toBe("pid:1:1");
    await vi.advanceTimersByTimeAsync(1);
    expect(store.copied).toBeNull();
    stop();
    vi.unstubAllGlobals();
  });
});

describe("startAll", () => {
  it("ends a start that resolves after the stop", async () => {
    const early = vi.fn();
    const late = vi.fn();
    let resolveLate: (stop: () => void) => void = () => {};
    const stopAll = startAll(
      async () => early,
      () => new Promise<() => void>((resolve) => (resolveLate = resolve)),
    );
    await Promise.resolve();
    stopAll();
    expect(early).toHaveBeenCalledOnce();
    expect(late).not.toHaveBeenCalled();
    resolveLate(late);
    await Promise.resolve();
    await Promise.resolve();
    expect(late).toHaveBeenCalledOnce();
  });
});
