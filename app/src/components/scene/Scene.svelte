<script lang="ts">
  // The scene element (AD-59, AD-61, AD-72): the five layers on one logical
  // box, scaled as one to the sessions area. It reads the board only, never
  // the store's clock (AD-64); the layout is derived from the board, and its
  // memory committed after each layout without re-running it (D-184).
  import type { Snippet } from "svelte";
  import { beaconPair } from "../../lib/alert";
  import type { Board } from "../../lib/generated/Board";
  import type { ProjectColor } from "../../lib/generated/ProjectColor";
  import { fitScale } from "../../lib/scene/geometry";
  import { layoutScene, liveCards, type SceneLayout } from "../../lib/scene/layout";
  import * as memory from "../../lib/scene/memory";
  import SceneFloor from "./SceneFloor.svelte";
  import SceneHalos from "./SceneHalos.svelte";
  import SceneObjects from "./SceneObjects.svelte";
  import SceneOverlay from "./SceneOverlay.svelte";

  interface Props {
    board: Board;
    /** Layer 5, story 4.8's targets. */
    targets?: Snippet<[SceneLayout]>;
  }
  let { board, targets }: Props = $props();

  // `load` reads storage once, before the first layout, and returns the
  // committed memory after that.
  let layout = $derived(layoutScene({ board }, memory.load(board)));
  $effect(() => memory.commit(layout.memory));

  let cards = $derived(new Map(liveCards(board).map((card) => [card.key, card])));
  let beacon = $derived(beaconPair(board));
  let colours = $derived(new Map<string, ProjectColor | null>(liveCards(board).map((card) => [card.project.name, card.project.color])));

  /** The sessions area's client size, the scrollbar's room left out. */
  let area = $state({ w: 0, h: 0 });
  let pixelRatio = $state(1);
  let scale = $derived(fitScale(area, layout.canvas));
  let width = $derived(Math.floor(layout.canvas.w * scale));
  let height = $derived(Math.floor(layout.canvas.h * scale));
  // A whole-pixel offset keeps the snapped figures on device pixels.
  let left = $derived(Math.max(0, Math.floor((area.w - width) / 2)));

  /** Follows the size of the element holding the scene. */
  function fit(node: HTMLElement) {
    const holder = node.parentElement;
    if (!holder) return;
    const measure = () => {
      const next = { w: holder.clientWidth, h: holder.clientHeight };
      if (next.w !== area.w || next.h !== area.h) area = next;
      pixelRatio = globalThis.devicePixelRatio || 1;
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(holder);
    return { destroy: () => observer.disconnect() };
  }
</script>

<!-- `data-scale` is written with the transform, in the same update (D-185);
     `data-moving` is story 4.10's mover's, "0" while nothing walks. -->
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
  use:fit
>
  <p class="summary" data-testid="scene-summary">{layout.summary}</p>
  <div class="stack" style:width="{layout.canvas.w}px" style:height="{layout.canvas.h}px" style:transform="scale({scale})">
    <SceneFloor {layout} />
    <SceneHalos {layout} {cards} {beacon} />
    <SceneObjects {layout} {colours} snap={scale * pixelRatio} />
    <SceneOverlay {layout} {cards} />
    {#if targets}
      <div class="targets">{@render targets(layout)}</div>
    {/if}
  </div>
</div>

<style>
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
