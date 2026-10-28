<script lang="ts">
  // Layer 1 (AD-59): the floor, its walls and what hangs on them, the room
  // floors, rugs and spot marks. Static for a column count; the decision
  // office's outline turns amber while someone waits there, and the alert
  // room's floor and hazard outline light while someone stands in it
  // (AD-175).
  import { SHARED_CELLS } from "../../lib/scene/geometry";
  import { ALERT_FLOOR, floorShapes, paint, roomOutline } from "../../lib/scene/furniture";
  import { alertLit, type SceneLayout } from "../../lib/scene/layout";

  interface Props {
    layout: SceneLayout;
  }
  let { layout }: Props = $props();

  // Recomputed when the floor grows, not on every board.
  let columns = $derived(layout.columns);
  let shapes = $derived(floorShapes(columns));
  let queueOn = $derived(layout.rooms.some((room) => room.kind === "decision" && room.plaque.count > 0));
  let alertOn = $derived(alertLit(layout));
</script>

<svg
  class="layer"
  aria-hidden="true"
  style:pointer-events="none"
  width={layout.canvas.w}
  height={layout.canvas.h}
  viewBox="0 0 {layout.canvas.w} {layout.canvas.h}"
  data-layer="floor"
>
  {#each shapes as s, i (i)}
    {#if s.kind === "polygon"}
      {#if s.alert === "floor"}
        <polygon
          data-alert-floor
          points={s.points}
          fill={alertOn ? ALERT_FLOOR.fill : s.fill}
          stroke={alertOn ? ALERT_FLOOR.stroke : s.stroke}
          stroke-width={s.strokeWidth}
          stroke-linejoin="round"
        />
      {:else if s.alert !== "hazard" || alertOn}
        <polygon
          data-hazard={s.alert === "hazard" ? "" : undefined}
          points={s.points}
          fill={s.fill}
          stroke={s.stroke}
          stroke-width={s.strokeWidth}
          stroke-dasharray={s.dash}
          stroke-linejoin={s.stroke ? "round" : undefined}
          opacity={s.opacity}
        />
      {/if}
    {:else if s.kind === "line"}
      <line
        x1={s.x1}
        y1={s.y1}
        x2={s.x2}
        y2={s.y2}
        stroke={s.stroke}
        stroke-width={s.strokeWidth}
        stroke-linecap={s.round ? "round" : undefined}
        opacity={s.opacity}
      />
    {:else if s.kind === "ellipse"}
      <ellipse
        cx={s.cx}
        cy={s.cy}
        rx={s.rx}
        ry={s.ry}
        fill={s.fill}
        stroke={s.stroke}
        stroke-width={s.strokeWidth}
        stroke-dasharray={s.dash}
        opacity={s.opacity}
      />
    {:else}
      <circle cx={s.cx} cy={s.cy} r={s.r} fill={s.fill} stroke={s.stroke} stroke-width={s.strokeWidth} />
    {/if}
  {/each}
  {#if queueOn}
    <polygon
      data-queue
      points={roomOutline(layout.columns, SHARED_CELLS.decision)}
      fill="none"
      stroke={paint("color-attention-border")}
      stroke-width="2"
      stroke-linejoin="round"
    />
  {/if}
</svg>
