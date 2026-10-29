<script lang="ts">
  // Layer 4 (AD-59, D-106): plaques, the "high priority" pills and the
  // door's sign, HTML inside `foreignObject` so names end in an ellipsis
  // within their caps. The register is overlay UI at a fixed size, outside
  // the scaled layers (`Register.svelte`, D-555). Session and project names
  // reach the page here as text only, never as markup.
  import { entranceSignRect, plaqueRect, priorityPillRect, type Rect } from "../../lib/scene/geometry";
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
</svg>
