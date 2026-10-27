import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import Gauge from "./Gauge.svelte";
import SessionCard from "./SessionCard.svelte";
import { BoardStore } from "../lib/board.svelte";
import type { Card } from "../lib/generated/Card";

const NOW = 1_800_000_000_000;

afterEach(cleanup);

function card(overrides: Partial<Card> = {}): Card {
  return {
    key: "pid:1:1",
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
    last_event_at_ms: NOW - 4 * 60_000,
    started_at_ms: NOW - 60 * 60_000,
    attach_available: false,
    ...overrides,
  };
}

function store(): BoardStore {
  const s = new BoardStore();
  s.nowMs = NOW;
  return s;
}

describe("SessionCard", () => {
  it("shows name, waiting reason and times", () => {
    const { getByText, getByTestId } = render(SessionCard, { card: card(), store: store() });
    expect(getByText("billing-api", { selector: ".name" })).toBeTruthy();
    expect(getByTestId("waiting-for").textContent).toBe("permission prompt");
    expect(getByText("4 min ago")).toBeTruthy();
    expect(getByText("running 1 h")).toBeTruthy();
  });

  it("renders session text as text, never as markup", () => {
    const hostile = `<img src=x onerror="alert(1)"><b>x</b>`;
    const { container, getByTestId } = render(SessionCard, {
      card: card({ last_prompt: hostile, last_message: hostile, name: hostile }),
      store: store(),
    });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    expect(getByTestId("last-prompt").textContent).toBe(hostile);
  });

  it("numbers Needs you cards for the keyboard and offers attach on background cards", () => {
    const { getByText, queryByText } = render(SessionCard, {
      card: card({ kind: "background", background_id: "abc", attach_available: true }),
      store: store(),
      index: 0,
    });
    expect(getByText("1")).toBeTruthy();
    expect(getByText("Copy attach command")).toBeTruthy();
    expect(queryByText("Wrong window?")).toBeNull();
  });

  it("shows the badge of an unknown state", () => {
    const { getByTestId } = render(SessionCard, {
      card: card({ column: "working", waiting_for: null, badge: "unknown" }),
      store: store(),
    });
    expect(getByTestId("badge").textContent).toBe("unknown");
  });
});

describe("Gauge", () => {
  const base = { label: "5 h", weekly: false, staleAfterMs: 30 * 60_000, nowMs: NOW };

  it("says there is no data instead of showing zero", () => {
    const { getByText } = render(Gauge, { ...base, window: null, writtenAtMs: null });
    expect(getByText("--")).toBeTruthy();
    expect(getByText("no data: see officina doctor")).toBeTruthy();
  });

  it("flags stale data with its age", () => {
    const { getByText } = render(Gauge, {
      ...base,
      window: { used_percentage: 62.4, resets_at: NOW / 1000 + 3600 },
      writtenAtMs: NOW - 45 * 60_000,
    });
    expect(getByText("62 %")).toBeTruthy();
    expect(getByText("stale, updated 45 min ago")).toBeTruthy();
  });

  it("never shows a percentage once the window has reset", () => {
    const { getByText } = render(Gauge, {
      ...base,
      window: { used_percentage: 90, resets_at: NOW / 1000 - 60 },
      writtenAtMs: NOW - 5 * 60_000,
    });
    expect(getByText("--")).toBeTruthy();
    expect(getByText("reset, no new data")).toBeTruthy();
  });
});
