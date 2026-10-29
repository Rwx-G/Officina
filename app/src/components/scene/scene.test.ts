import { afterEach, describe, expect, it, vi } from "vitest";
import { tick } from "svelte";
import { cleanup, render } from "@testing-library/svelte";
import Scene from "./Scene.svelte";
import { BoardStore } from "../../lib/board.svelte";
import type { Board } from "../../lib/generated/Board";
import type { Card } from "../../lib/generated/Card";
import { beaconPair } from "../../lib/alert";
import { HALO_BREATHING_MS, haloMemo } from "../../lib/halo";
import { paint } from "../../lib/scene/furniture";
import { layoutScene, type SceneLayout } from "../../lib/scene/layout";
import * as memory from "../../lib/scene/memory";
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

function renderScene(b: Board, walking?: boolean) {
  return render(Scene, { board: b, store: new BoardStore(), onBoard: () => {}, walking });
}

/** Renders `b` and returns the layout the scene drew it from: the one the
 *  run's memory gives, as `Scene.svelte` lays it out. */
function renderWithLayout(b: Board): { container: HTMLElement; layout: SceneLayout; unmount: () => void } {
  const layout = layoutScene({ board: b }, memory.load(b));
  const { container, unmount } = renderScene(b);
  return { container, layout, unmount };
}

const buttonsOf = (container: HTMLElement) => [...container.querySelectorAll<HTMLButtonElement>('[data-layer="targets"] button')];

describe("Scene", () => {
  it("layers 1 to 4 and the badges are hidden from assistive technology and ignore the pointer", () => {
    const { getByTestId } = renderScene(waitingBoard());
    const scene = getByTestId("scene");
    expect(scene.getAttribute("tabindex")).toBe("-1");
    expect(scene.hasAttribute("role")).toBe(false);
    expect(scene.getAttribute("data-moving")).toBe("0");
    const all = [...scene.querySelectorAll("[data-layer]")];
    expect(all.map((l) => l.getAttribute("data-layer"))).toEqual(["floor", "halos", "objects", "badges", "overlay", "targets"]);
    const layers = all.slice(0, -1);
    for (const layer of layers) {
      expect(layer.getAttribute("aria-hidden"), layer.getAttribute("data-layer")!).toBe("true");
      expect((layer as HTMLElement).style.pointerEvents, layer.getAttribute("data-layer")!).toBe("none");
    }
    // Layer 5 carries the names (AD-59).
    expect(all.at(-1)!.hasAttribute("aria-hidden")).toBe(false);
    // The text alternative is inside the scene element, outside the layers.
    const summary = getByTestId("scene-summary");
    expect(scene.contains(summary)).toBe(true);
    expect(all.some((l) => l.contains(summary))).toBe(false);
    expect(summary.textContent).toMatch(/need/);
  });

  it("plaque and register names render as text", () => {
    const markup = '<img src="x" onerror="globalThis.__pwned = 1"><b>bold</b>';
    const waiting = card(markup, "decision", { project: markup });
    const b = board([waiting], { floor_projects: [project(markup)] });
    const { getByTestId } = renderScene(b);
    const scene = getByTestId("scene");
    expect(scene.querySelector("img, b, script")).toBeNull();
    // The scene's memory lives for the run, here this file's: the project
    // takes the next free office, whichever it is.
    const plaques = [...scene.querySelectorAll("[data-plaque]")].map((p) => p.textContent ?? "");
    expect(plaques.some((text) => text.includes(markup))).toBe(true);
    expect(getByTestId("register").textContent).toContain(markup);
    // Layer 3 and the badges hold no text but digits.
    for (const name of ["objects", "badges"]) {
      const walker = document.createTreeWalker(scene.querySelector(`[data-layer="${name}"]`)!, NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) expect(n.textContent!.trim()).toMatch(/^\d*$/);
    }
    // Targets and their name tags carry the name as text.
    const target = buttonsOf(scene).find((b) => b.dataset.key === waiting.key)!;
    expect(target.getAttribute("aria-label")).toContain(markup);
    expect(target.querySelector("[data-tag]")!.textContent).toContain(markup);
    expect((globalThis as { __pwned?: number }).__pwned).toBeUndefined();
  });

  it("one halo per employee in the decision office or the alert room", () => {
    for (const { name, board: b } of fixtureBoards()) {
      const { container, unmount } = renderScene(b);
      const layout = layoutScene({ board: b }, emptyMemory(b.floor));
      const expected = layout.employees.filter((e) => (e.zone === "decision" || e.zone === "alert") && e.at !== null).map((e) => e.key);
      const halos = [...container.querySelectorAll("[data-halo]")].map((h) => h.getAttribute("data-halo"));
      expect(halos.sort(), name).toEqual(expected.sort());
      unmount();
    }
  });

  it("a halo in the alert room uses the alert colour", () => {
    const b = board([card("queued", "decision"), card("alerted", "alert", { started: 1 })]);
    const { container } = renderScene(b);
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
    const { container, unmount } = renderScene(b);
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
    const quiet = renderScene(board([card("queued", "decision")]));
    expect(quiet.container.querySelector("[data-beacon]")).toBeNull();
  });

  it("one button per target with its accessible name", () => {
    for (const { name, board: b } of fixtureBoards()) {
      const { container, layout, unmount } = renderWithLayout(b);
      const buttons = buttonsOf(container);
      expect(buttons.length, name).toBe(layout.targets.length);
      for (const target of layout.targets) {
        const button = buttons.find((x) => x.dataset.target === target.id);
        expect(button, `${name} ${target.id}`).toBeDefined();
        expect(button!.getAttribute("aria-label"), target.id).toBe(target.label);
        expect(button!.type).toBe("button");
        if (target.kind === "figure") expect(button!.dataset.key, target.id).toBe(target.keys[0]);
        else expect(button!.hasAttribute("data-key"), target.id).toBe(false);
        // At its rectangle, in logical px under the shared transform.
        expect([button!.style.left, button!.style.top, button!.style.width, button!.style.height], target.id).toEqual(
          [target.rect.x, target.rect.y, target.rect.w, target.rect.h].map((v) => `${v}px`),
        );
      }
      unmount();
    }
  });

  it("DOM order equals the layout's tab order", () => {
    for (const { name, board: b } of fixtureBoards()) {
      const { container, layout, unmount } = renderWithLayout(b);
      expect(buttonsOf(container).map((x) => x.dataset.target), name).toEqual(layout.targets.map((t) => t.id));
      // No target takes a tab index of its own: DOM order is the order.
      for (const button of buttonsOf(container)) expect(button.hasAttribute("tabindex")).toBe(false);
      unmount();
    }
  });

  it("the name tag is in the button before any hover and ignores the pointer", () => {
    for (const { name, board: b } of fixtureBoards()) {
      const { container, layout, unmount } = renderWithLayout(b);
      for (const button of buttonsOf(container)) {
        const tag = button.querySelector<HTMLElement>("[data-tag]");
        expect(tag, `${name} ${button.dataset.target}`).not.toBeNull();
        expect(tag!.textContent).toBe(layout.targets.find((t) => t.id === button.dataset.target)!.tag);
        expect(tag!.style.pointerEvents).toBe("none");
        expect(tag!.getAttribute("aria-hidden")).toBe("true");
      }
      unmount();
    }
  });

});

describe("Scene with walking", () => {
  /** Motion allowed (the setup's default is reduced), the frames and the
   *  page clock driven by the test. */
  function moving(): () => void {
    const reduced = window.matchMedia;
    window.matchMedia = (query: string) => ({ ...reduced(query), matches: false });
    vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame", "performance"] });
    return () => {
      vi.useRealTimers();
      window.matchMedia = reduced;
    };
  }

  /** Steps frames until the scene settles; returns how many it took. */
  async function settle(scene: HTMLElement): Promise<number> {
    let frames = 0;
    while (scene.getAttribute("data-moving") === "1" && frames < 2_000) {
      vi.advanceTimersByTime(16);
      frames++;
    }
    await tick();
    return frames;
  }

  it("a walking employee has no halo until it arrives", async () => {
    const restore = moving();
    try {
      // From the lounge, which has a place whatever offices the run holds.
      const other = card("walk-other", "office", { project: "walk-p" });
      const { getByTestId, container, rerender } = renderScene(board([card("walker", "lounge", { project: "walk-p" }), other]), true);
      const scene = getByTestId("scene");
      const halo = () => container.querySelector<HTMLElement>('[data-halo="walker"]');
      await tick();
      expect(scene.getAttribute("data-moving")).toBe("0");
      expect(halo()).toBeNull();

      // To the decision office: no halo on the way, one on arrival.
      await rerender({ board: board([card("walker", "decision", { project: "walk-p" }), other]) });
      await tick();
      expect(scene.getAttribute("data-moving")).toBe("1");
      expect(halo()).toBeNull();
      vi.advanceTimersByTime(500);
      await tick();
      expect(halo()).toBeNull();
      expect(await settle(scene)).toBeGreaterThan(10);
      expect(halo()).not.toBeNull();
      const count = halo()!.style.getPropertyValue("--halo-count");

      // On to the alert room: the halo goes while it walks and comes back
      // with the same wait's timing, the memo's (key, enteredAt).
      await rerender({ board: board([card("walker", "alert", { project: "walk-p", alert: "priority" }), other]) });
      await tick();
      expect(scene.getAttribute("data-moving")).toBe("1");
      expect(halo()).toBeNull();
      await settle(scene);
      expect(halo()).not.toBeNull();
      expect(halo()!.style.getPropertyValue("--halo-colour")).toBe(paint("color-alert"));
      expect(halo()!.style.getPropertyValue("--halo-count")).toBe(count);
    } finally {
      restore();
    }
  });
});
