<script lang="ts">
  // Layer 4 (AD-59, D-106): plaques, the "high priority" pills and the
  // door's sign, HTML inside `foreignObject` so names end in an ellipsis
  // within their caps. The register is overlay UI at a fixed size, outside
  // the scaled layers (`Register.svelte`, D-555). Session and project names
  // reach the page here as text only, never as markup.
  // While the orchestrator is live (D-567), the decider's two screens and
  // the whiteboard carry words in their own planes: the pending proposals
  // and the Needs you count, the latest proposals' labels. They are
  // redrawn when those change, never on a clock, and nothing scrolls.
  import { deciderScreens, entranceSignRect, plaqueRect, priorityPillRect, whiteboardPlane, type Plane, type Rect } from "../../lib/scene/geometry";
  import type { Room, SceneLayout } from "../../lib/scene/layout";
  import { SIGN_WORDS } from "../../lib/scene/text";
  import Plaque from "./Plaque.svelte";

  interface Props {
    layout: SceneLayout;
  }
  let { layout }: Props = $props();

  /** The plaque's rectangle: More offices hangs its plaque where the
   *  office in its cell would. */
  const plaqueOf = (room: Room): Rect => plaqueRect(layout.columns, room.cell, room.kind === "more" ? "office" : room.kind);

  let pills = $derived(layout.rooms.filter((room) => room.slot !== null && room.plaque.highPriority));
  let entrance = $derived(entranceSignRect(layout.columns));

  let display = $derived(layout.display);
  let screens = $derived(deciderScreens(layout.columns));
  let board = $derived(whiteboardPlane(layout.columns));
  /** What each screen shows, left then right. */
  let counts = $derived(display ? [display.proposals, display.needsYou] : []);
  const matrix = (plane: Plane) => `matrix(${plane.matrix.join(" ")})`;
</script>

<svg
  class="layer"
  aria-hidden="true"
  style:pointer-events="none"
  width={layout.canvas.w}
  height={layout.canvas.h}
  viewBox="0 0 {layout.canvas.w} {layout.canvas.h}"
  data-layer="overlay"
>
  {#each layout.rooms as room (room.id)}
    {@const r = plaqueOf(room)}
    <foreignObject x={r.x} y={r.y} width={r.w} height={r.h} data-plaque={room.id}>
      <Plaque
        text={room.plaque.text}
        state={room.plaque.state}
        count={room.plaque.count}
        dot={room.kind === "office" ? (room.project?.color ?? null) : undefined}
        more={room.plaque.more}
        moreAtEnd={room.kind === "decision"}
      />
    </foreignObject>
  {/each}
  {#each pills as room (room.id)}
    {@const r = priorityPillRect(layout.columns, room.cell)}
    <foreignObject x={r.x} y={r.y} width={r.w} height={r.h} data-priority={room.id}>
      <Plaque text={SIGN_WORDS.highPriority} state="alert" count={null} small />
    </foreignObject>
  {/each}
  <foreignObject x={entrance.x} y={entrance.y} width={entrance.w} height={entrance.h} data-sign="entrance">
    <Plaque text={SIGN_WORDS.entrance} state="quiet" count={null} small />
  </foreignObject>
  {#if display}
    {#each counts as count, i (i)}
      {@const plane = screens[i]!}
      <foreignObject width={plane.w} height={plane.h} transform={matrix(plane)} data-display-screen={i === 0 ? "proposals" : "needs-you"}>
        <div class="screen">{count}</div>
      </foreignObject>
    {/each}
    <foreignObject width={board.w} height={board.h} transform={matrix(board)} data-whiteboard>
      <ul class="whiteboard">
        {#each display.labels as label, i (i)}
          <li data-label>{label}</li>
        {/each}
      </ul>
    </foreignObject>
  {/if}
</svg>

<style>
  /* A lit screen, as an office's while its session works there. */
  .screen {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    background: var(--color-working);
    color: var(--color-surface);
    font-size: 9px;
    font-weight: 800;
    line-height: 1;
  }
  /* The board's own white, over v0.4's scribbles; one label a line, cut
     with an ellipsis (D-106). */
  .whiteboard {
    display: flex;
    flex-direction: column;
    gap: 1.5px;
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    margin: 0;
    padding: 3px 4px;
    list-style: none;
    background: var(--scene-board);
    color: var(--scene-metal);
    font-size: 9.5px;
    font-weight: 700;
    line-height: 1.1;
  }
  .whiteboard li {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
</style>
