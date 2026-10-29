<script lang="ts">
  // The scene element (AD-59, AD-61, AD-72): the five layers on one logical
  // box, scaled as one to the sessions area. It reads the board only, never
  // the store's clock (AD-64); the layout is derived from the board, and its
  // memory committed after each layout without re-running it (D-184). It
  // hosts the mover (AD-172), between the layout and layer 3.
  // Over the scene's corner, outside the scaled layers: the register, at a
  // fixed size (D-555).
  // Beside the scene element, outside it (D-111): the file panel, which this
  // component opens, closes and returns focus from (AD-63). The counts of
  // ended sessions are in the header (D-541).
  import { onDestroy, tick } from "svelte";
  import type { BoardStore } from "../../lib/board.svelte";
  import { beaconPair } from "../../lib/alert";
  import type { Board } from "../../lib/generated/Board";
  import type { Card } from "../../lib/generated/Card";
  import type { ProjectColor } from "../../lib/generated/ProjectColor";
  import { fitScale } from "../../lib/scene/geometry";
  import { layoutScene, liveCards, type Target } from "../../lib/scene/layout";
  import * as memory from "../../lib/scene/memory";
  import { Mover } from "../../lib/scene/mover";
  import { DEFAULT_WALKING } from "../../lib/view.svelte";
  import { meeting } from "../../lib/meeting.svelte";
  import SessionFile, { type FileForm } from "../SessionFile.svelte";
  import SceneFloor from "./SceneFloor.svelte";
  import SceneHalos from "./SceneHalos.svelte";
  import SceneObjects from "./SceneObjects.svelte";
  import SceneOverlay from "./SceneOverlay.svelte";
  import SceneTargets from "./SceneTargets.svelte";
  import Register, { type RegisterRow } from "./Register.svelte";

  interface Props {
    board: Board;
    /** What the file's actions act through, as the board cards do. */
    store: BoardStore;
    /** Switches the Sessions view to Board mode. */
    onBoard: () => void;
    /** Figures walk to their places, or glide there (AD-66). */
    walking?: boolean;
  }
  let { board, store, onBoard, walking = DEFAULT_WALKING }: Props = $props();

  // One mover per mounted scene: its first layout places everyone at once,
  // so a new run or a mode switch never makes the company walk in (D-469).
  const mover = new Mover();
  /** The keys on their way to a place: no halo until they arrive (AD-65). */
  let transit = $state<ReadonlySet<string>>(new Set());
  mover.onTransit = (keys) => (transit = keys);
  onDestroy(() => mover.destroy());

  /** Hands the scene element to the mover, which keeps `data-moving`. */
  function host(node: HTMLElement) {
    mover.attach(node);
  }

  // `load` reads storage once, before the first layout, and returns the
  // committed memory after that.
  // The open meeting's figure is marked received wherever it stands
  // (D-566): opening and closing a meeting each change the scene once.
  let layout = $derived(layoutScene({ board, received: meeting.view?.key ?? meeting.key }, memory.load(board)));
  $effect(() => memory.commit(layout.memory));

  let cards = $derived(new Map(liveCards(board).map((card) => [card.key, card])));
  let beacon = $derived(beaconPair(board));
  /** Who waits, in board order: the register's rows. */
  let waiting = $derived<RegisterRow[]>(
    layout.employees
      .filter((e) => e.zone === "decision" || e.zone === "alert")
      .map((e) => ({ key: e.key, name: cards.get(e.key)?.name ?? e.key, number: e.marks.ticket, approval: e.marks.approval, alert: e.zone === "alert" })),
  );
  let colours = $derived(new Map<string, ProjectColor | null>(liveCards(board).map((card) => [card.project.name, card.project.color])));

  /** The sessions area's client size, the scrollbar's room left out. */
  let area = $state({ w: 0, h: 0 });
  let pixelRatio = $state(1);
  let scale = $derived(fitScale(area, layout.canvas));
  let width = $derived(Math.floor(layout.canvas.w * scale));
  let height = $derived(Math.floor(layout.canvas.h * scale));
  /** Where the session file's left edge stands in the area, from
   *  app.css's `--file-width` and `--file-inset` (D-702): the panel is
   *  placed from the area's outer right edge, its scrollbar's room
   *  included. */
  let fileLeft = $state(0);
  /** Centred; where the area leaves room beside the scene for the file,
   *  far enough left that the file docks there instead of covering it
   *  (D-543). A whole-pixel offset keeps the snapped figures on device
   *  pixels. */
  let left = $derived.by(() => {
    const centred = Math.max(0, Math.floor((area.w - width) / 2));
    const docked = fileLeft - width;
    return docked >= 0 ? Math.min(centred, Math.floor(docked)) : centred;
  });

  /** Follows the size of the element holding the scene. */
  function fit(node: HTMLElement) {
    const holder = node.parentElement;
    if (!holder) return;
    const measure = () => {
      const next = { w: holder.clientWidth, h: holder.clientHeight };
      if (next.w !== area.w || next.h !== area.h) area = next;
      pixelRatio = globalThis.devicePixelRatio || 1;
      const style = getComputedStyle(holder);
      const length = (name: string) => parseFloat(style.getPropertyValue(name)) || 0;
      fileLeft = holder.offsetWidth - length("--file-inset") - length("--file-width");
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(holder);
    return { destroy: () => observer.disconnect() };
  }

  // ------------------------------------------------------------ the file

  /** What the file panel holds: a session's file, by key, opened from a
   *  figure or from a group's list; or a group's list, by its target. */
  type Open = { kind: "file"; key: string; from: string | null } | { kind: "list"; target: string; focusKey: string | null };
  let open = $state<Open | null>(null);

  let sceneElement: HTMLElement;
  let targetsLayer: ReturnType<typeof SceneTargets>;

  /** Every card of the board by key: a file follows its card into any
   *  column, and closes only when the card leaves the board (D-111). */
  let allCards = $derived(new Map<string, Card>(Object.values(board.columns).flatMap((list: Card[]) => list.map((c) => [c.key, c] as const))));

  let form = $derived.by<FileForm | null>(() => {
    if (!open) return null;
    if (open.kind === "file") {
      const card = allCards.get(open.key);
      return card ? { kind: "file", card, back: open.from !== null } : null;
    }
    const opened = open;
    const target = layout.targets.find((t) => t.id === opened.target);
    if (!target) return null;
    const room = layout.rooms.find((r) => r.id === target.group);
    return {
      kind: "list",
      title: room?.plaque.text ?? target.label,
      cards: target.keys.flatMap((key) => allCards.get(key) ?? []),
      focusKey: opened.focusKey,
    };
  });
  /** Remounts the panel, which focuses its first control, on each new
   *  file or list, never on a board update of the one shown. */
  let formId = $derived(open === null ? "" : open.kind === "file" ? `file:${open.key}` : `list:${open.target}`);

  // The card left the board, or the group emptied: the panel closes, and
  // focus, which it held or which nothing holds, goes to the scene element.
  $effect(() => {
    if (open !== null && form === null) {
      open = null;
      if (document.activeElement === null || document.activeElement === document.body) sceneElement.focus();
    }
  });

  // One panel at a time (D-111): a meeting takes the file's slot.
  $effect(() => {
    if (meeting.key !== null) open = null;
  });

  function activate(target: Target): void {
    meeting.close(false);
    open = target.kind === "group" ? { kind: "list", target: target.id, focusKey: null } : { kind: "file", key: target.keys[0]!, from: null };
  }

  /** The target that now represents `key`: its figure, else its group. */
  function representing(key: string): string | null {
    const holding = layout.targets.filter((t) => t.kind !== "helper" && t.keys.includes(key));
    return (holding.find((t) => t.kind === "figure") ?? holding[0])?.id ?? null;
  }

  /** Escape: from a listed file to its list, else closed, focus on the
   *  target that now represents what was shown, else the scene element. */
  async function back(): Promise<void> {
    const was = open;
    if (!was) return;
    if (was.kind === "file" && was.from !== null && layout.targets.some((t) => t.id === was.from)) {
      open = { kind: "list", target: was.from, focusKey: was.key };
      return;
    }
    open = null;
    const id = was.kind === "file" ? representing(was.key) : was.target;
    await tick();
    if (id === null || !targetsLayer.focusTarget(id)) sceneElement.focus();
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key !== "Escape" || open === null || event.ctrlKey || event.altKey || event.metaKey) return;
    event.preventDefault();
    void back();
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- The sessions area's scroller: below the minimum area it scrolls; the
     file panel stays at its right edge. -->
<div class="area" data-testid="scene-area">
  <!-- `data-scale` is written with the transform, in the same update (D-185);
       `data-moving` starts at "0" and is the mover's after that. -->
  <div
    class="scene"
    data-testid="scene"
    tabindex="-1"
    data-scale={scale}
    data-columns={layout.columns}
    data-moving="0"
    style:width="{width}px"
    style:height="{height}px"
    style:margin-left="{left}px"
    bind:this={sceneElement}
    use:fit
    use:host
  >
    <p class="summary" data-testid="scene-summary">{layout.summary}</p>
    <div class="stack" style:width="{layout.canvas.w}px" style:height="{layout.canvas.h}px" style:transform="scale({scale})">
      <SceneFloor {layout} />
      <SceneHalos {layout} {cards} {beacon} {transit} />
      <SceneObjects {layout} {colours} snap={scale * pixelRatio} {mover} {walking} />
      <SceneOverlay {layout} />
      <SceneTargets {layout} onActivate={activate} bind:this={targetsLayer} />
    </div>
    {#if waiting.length > 0}
      <Register rows={waiting} />
    {/if}
  </div>
</div>
{#if form}
  {#key formId}
    <SessionFile {form} {board} {store} onClose={() => void back()} onOpen={(key) => (open = { kind: "file", key, from: open?.kind === "list" ? open.target : null })} {onBoard} />
  {/key}
{/if}

<style>
  .area {
    position: absolute;
    inset: 0;
    overflow: auto;
    /* The scrollbar's room is kept, so the scene's own scale never decides
       whether the area scrolls. */
    scrollbar-gutter: stable;
  }
  .scene {
    position: relative;
    overflow: hidden;
    outline: none;
  }
  .stack {
    position: absolute;
    top: 0;
    left: 0;
    transform-origin: 0 0;
  }
  .stack > :global(*) {
    position: absolute;
    inset: 0;
  }
  /* Visually hidden, read by assistive technology (AD-63). */
  .summary {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
    border: 0;
  }
</style>
