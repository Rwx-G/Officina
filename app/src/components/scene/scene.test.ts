import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import Scene from "./Scene.svelte";
import type { Board } from "../../lib/generated/Board";
import type { Card } from "../../lib/generated/Card";
import { beaconPair } from "../../lib/alert";
import { HALO_BREATHING_MS, haloMemo } from "../../lib/halo";
import { paint } from "../../lib/scene/furniture";
import { layoutScene } from "../../lib/scene/layout";
import { emptyMemory } from "../../lib/scene/memory";
import { board, card, fixtureBoards, project } from "../../lib/scene/testing";

afterEach(cleanup);

// The scene's memory is one module variable per run (D-184): the cases
// below share it, in order, as one window would.

/** A fixture board with someone waiting in the decision office. */
function waitingBoard(): Board {
  const found = fixtureBoards().find(({ board: b }) => b.columns.needs_you.some((c) => c.scene?.zone === "decision"));
  if (!found) throw new Error("no fixture board with a decision office queue");
  return found.board;
}

describe("Scene", () => {
  it("layers 1 to 4 are hidden from assistive technology and ignore the pointer", () => {
    const { getByTestId } = render(Scene, { board: waitingBoard() });
    const scene = getByTestId("scene");
    expect(scene.getAttribute("tabindex")).toBe("-1");
    expect(scene.hasAttribute("role")).toBe(false);
    expect(scene.getAttribute("data-moving")).toBe("0");
    const layers = [...scene.querySelectorAll("[data-layer]")];
    expect(layers.map((l) => l.getAttribute("data-layer"))).toEqual(["floor", "halos", "objects", "overlay"]);
    for (const layer of layers) {
      expect(layer.getAttribute("aria-hidden"), layer.getAttribute("data-layer")!).toBe("true");
      expect((layer as HTMLElement).style.pointerEvents, layer.getAttribute("data-layer")!).toBe("none");
    }
    // The text alternative is inside the scene element, outside the layers.
    const summary = getByTestId("scene-summary");
    expect(scene.contains(summary)).toBe(true);
    expect(layers.some((l) => l.contains(summary))).toBe(false);
    expect(summary.textContent).toMatch(/need/);
  });

  it("plaque and register names render as text", () => {
    const markup = '<img src="x" onerror="globalThis.__pwned = 1"><b>bold</b>';
    const waiting = card(markup, "decision", { project: markup });
    const b = board([waiting], { floor_projects: [project(markup)] });
    const { getByTestId } = render(Scene, { board: b });
    const scene = getByTestId("scene");
    expect(scene.querySelector("img, b, script")).toBeNull();
    // The scene's memory lives for the run, here this file's: the project
    // takes the next free office, whichever it is.
    const plaques = [...scene.querySelectorAll("[data-plaque]")].map((p) => p.textContent ?? "");
    expect(plaques.some((text) => text.includes(markup))).toBe(true);
    expect(getByTestId("register").textContent).toContain(markup);
    // Layer 3 holds no text but digits.
    const objects = scene.querySelector('[data-layer="objects"]')!;
    const walker = document.createTreeWalker(objects, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) expect(n.textContent!.trim()).toMatch(/^\d*$/);
    expect((globalThis as { __pwned?: number }).__pwned).toBeUndefined();
  });

  it("one halo per employee in the decision office or the alert room", () => {
    for (const { name, board: b } of fixtureBoards()) {
      const { container, unmount } = render(Scene, { board: b });
      const layout = layoutScene({ board: b }, emptyMemory(b.floor));
      const expected = layout.employees.filter((e) => (e.zone === "decision" || e.zone === "alert") && e.at !== null).map((e) => e.key);
      const halos = [...container.querySelectorAll("[data-halo]")].map((h) => h.getAttribute("data-halo"));
      expect(halos.sort(), name).toEqual(expected.sort());
      unmount();
    }
  });

  it("a halo in the alert room uses the alert colour", () => {
    const b = board([card("queued", "decision"), card("alerted", "alert", { started: 1 })]);
    const { container } = render(Scene, { board: b });
    const colour = (key: string) => container.querySelector<HTMLElement>(`[data-halo="${key}"]`)!.style.getPropertyValue("--halo-colour");
    expect(colour("alerted")).toBe(paint("color-alert"));
    expect(colour("queued")).toBe(paint("color-attention"));
  });

  it("the beacon is keyed on the first alerted card", () => {
    // The memo's clock: the scene's modules read none (AD-64).
    const now = haloMemo.now();
    /** An alerted card whose wait began `sinceMs` ago, its last event long
     *  before: only a beacon keyed on the wait's start breathes. */
    const alerted = (key: string, sinceMs: number | null, started: number): Card => {
      const c = card(key, "alert", { started, alert: "priority" });
      return { ...c, last_event_at_ms: now - 10 * HALO_BREATHING_MS, scene: { ...c.scene!, waiting_since_ms: sinceMs === null ? null : now - sinceMs } };
    };
    const first = alerted("first", 1_000, 1);
    const b = board([card("queued", "decision"), first, alerted("second", 2_000, 2)]);
    expect(beaconPair(b)).toEqual({ key: "first", enteredAt: first.scene!.waiting_since_ms });
    const { container, unmount } = render(Scene, { board: b });
    const beacons = container.querySelectorAll<HTMLElement>("[data-beacon]");
    expect(beacons).toHaveLength(1);
    expect(beacons[0]!.getAttribute("data-beacon")).toBe("first");
    const timing = haloMemo.timing("first", first.scene!.waiting_since_ms);
    expect(timing.iterations).toBeGreaterThan(0);
    expect(beacons[0]!.style.getPropertyValue("--halo-count")).toBe(String(timing.iterations));
    unmount();
    // The start unknown: the beacon counts from when the card's halo does.
    const unknownStart = alerted("unknown-start", null, 1);
    expect(beaconPair(board([unknownStart]))).toEqual({ key: "unknown-start", enteredAt: unknownStart.last_event_at_ms });
    // A wait that reached the threshold: the beacon counts from that moment,
    // so a fresh alert breathes even though its wait is older than two
    // minutes (D-527).
    const waited = { ...alerted("waited", 30 * 60_000, 1), scene: { ...first.scene!, alert: "waited" as const, waiting_since_ms: now - 30 * 60_000 } };
    const late = { ...board([waited]), alert_after_ms: 30 * 60_000 - 1_000 };
    expect(beaconPair(late)).toEqual({ key: "waited", enteredAt: now - 1_000 });
    expect(haloMemo.timing("waited", now - 1_000).iterations).toBeGreaterThan(0);
    // No one in alert: no beacon.
    const quiet = render(Scene, { board: board([card("queued", "decision")]) });
    expect(quiet.container.querySelector("[data-beacon]")).toBeNull();
  });
});
