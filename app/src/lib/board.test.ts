import { afterEach, describe, expect, it, vi } from "vitest";
import { BoardStore, COPIED_VISIBLE_MS, OUTCOME_VISIBLE_MS } from "./board.svelte";
import { startAll } from "./lifecycle";
import { haloMemo } from "./halo";
import * as ipc from "./ipc";
import { fixturesOf } from "../../e2e/fixture-index";
import type { Board } from "./generated/Board";
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
    project: { name: "billing-api", color: "blue", priority: "normal" },
    session_id: "s",
    background_id: null,
    pid: 1,
    cwd: "C:/work/billing-api",
    waiting_for: "permission prompt",
    last_prompt: null,
    last_message: null,
    last_message_tail: null,
    last_event_at_ms: 0,
    started_at_ms: 0,
    attach_available: true,
    subject: null,
    scene: null,
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

describe("BoardStore halo timings", () => {
  afterEach(() => {
    haloMemo.retain([]);
    vi.restoreAllMocks();
  });

  /** `card`'s scene state once it works in its office. */
  const working = (card: Card): Card["scene"] => (card.scene ? { ...card.scene, zone: "office", alert: null, approval: false } : null);

  /** A real board (D-181) with at least two sessions waiting. */
  function boardWithWaits(): Board {
    const found = fixturesOf<Board>("board").find(({ data }) => data.columns.needs_you.length >= 2);
    expect(found, "a fixture board with two Needs you cards").toBeDefined();
    return found!.data;
  }

  it("a board drops the halo timings of sessions no longer waiting", async () => {
    const board = boardWithWaits();
    const [staying, leaving] = board.columns.needs_you;
    let push: (board: Board) => void = () => {};
    vi.mocked(ipc.onBoard).mockImplementationOnce(async (handler) => {
      push = handler;
      return unlisten;
    });
    vi.mocked(ipc.getBoard).mockResolvedValueOnce(board);
    const now = vi.spyOn(Date, "now").mockReturnValue(board.generated_at_ms);
    const store = new BoardStore();
    const stop = await store.start();
    const entered = (c: Card) => c.last_event_at_ms ?? c.started_at_ms;
    const timings = [staying!, leaving!].map((c) => haloMemo.timing(c.key, entered(c)));

    // The next board: one session answered, now Working in its office.
    const next: Board = {
      ...board,
      columns: {
        ...board.columns,
        needs_you: board.columns.needs_you.filter((c) => c.key !== leaving!.key),
        working: [...board.columns.working, { ...leaving!, column: "working", scene: working(leaving!) }],
      },
    };
    now.mockReturnValue(board.generated_at_ms + 60_000);
    push(next);
    const reads = now.mock.calls.length;
    expect(haloMemo.timing(staying!.key, entered(staying!))).toEqual(timings[0]);
    expect(now).toHaveBeenCalledTimes(reads);
    // The one that left is a new wait if it comes back: seen anew.
    haloMemo.timing(leaving!.key, entered(leaving!));
    expect(now).toHaveBeenCalledTimes(reads + 1);
    stop();
  });

  it("a session waiting on a relay approval alone keeps its halo timing", async () => {
    const board = boardWithWaits();
    const waiter = board.columns.needs_you[0]!;
    let push: (board: Board) => void = () => {};
    vi.mocked(ipc.onBoard).mockImplementationOnce(async (handler) => {
      push = handler;
      return unlisten;
    });
    vi.mocked(ipc.getBoard).mockResolvedValueOnce(board);
    const now = vi.spyOn(Date, "now").mockReturnValue(board.generated_at_ms);
    const store = new BoardStore();
    const stop = await store.start();
    const enteredAt = waiter.last_event_at_ms ?? waiter.started_at_ms;
    const timing = haloMemo.timing(waiter.key, enteredAt);
    // Still Working, its run blocked on an approval: the decision office.
    const approvalOnly: Card = { ...waiter, column: "working", scene: { ...waiter.scene!, zone: "decision", approval: true } };
    push({
      ...board,
      columns: {
        ...board.columns,
        needs_you: board.columns.needs_you.filter((c) => c.key !== waiter.key),
        working: [...board.columns.working, approvalOnly],
      },
    });
    now.mockReturnValue(board.generated_at_ms + 60_000);
    const reads = now.mock.calls.length;
    expect(haloMemo.timing(waiter.key, enteredAt)).toEqual(timing);
    expect(now).toHaveBeenCalledTimes(reads);
    stop();
  });

  it("the first board keeps the timings of what it shows waiting only", async () => {
    const board = boardWithWaits();
    vi.mocked(ipc.getBoard).mockResolvedValueOnce(board);
    const now = vi.spyOn(Date, "now").mockReturnValue(board.generated_at_ms);
    haloMemo.timing("pid:999:999", null);
    const store = new BoardStore();
    const stop = await store.start();
    const reads = now.mock.calls.length;
    haloMemo.timing("pid:999:999", null);
    expect(now).toHaveBeenCalledTimes(reads + 1);
    stop();
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
