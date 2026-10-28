<script lang="ts">
  // Layer 4 (AD-59, D-106): plaques, the "high priority" pills, the door's
  // sign and the register, HTML inside `foreignObject` so names end in an
  // ellipsis within their caps. Session and project names reach the page
  // here as text only, never as markup.
  import type { Card } from "../../lib/generated/Card";
  import {
    REGISTER,
    entranceSignRect,
    plaqueRect,
    priorityPillRect,
    type Rect,
  } from "../../lib/scene/geometry";
  import type { Room, SceneLayout } from "../../lib/scene/layout";
  import { SIGN_WORDS } from "../../lib/scene/text";
  import Plaque from "./Plaque.svelte";
  import Register, { type RegisterRow } from "./Register.svelte";

  interface Props {
    layout: SceneLayout;
    cards: ReadonlyMap<string, Card>;
  }
  let { layout, cards }: Props = $props();

  /** The plaque's rectangle: More offices hangs its plaque where the
   *  office in its cell would. The decision office's group is the
   *  register's last line, where its target sits (D-524), not a "+n". */
  const plaqueOf = (room: Room): Rect => plaqueRect(layout.columns, room.cell, room.kind === "more" ? "office" : room.kind);

  let pills = $derived(layout.rooms.filter((room) => room.slot !== null && room.plaque.highPriority));
  let rows = $derived<RegisterRow[]>(
    layout.employees
      .filter((e) => e.zone === "decision" || e.zone === "alert")
      .map((e) => ({ key: e.key, name: cards.get(e.key)?.name ?? e.key, number: e.marks.ticket, alert: e.zone === "alert" })),
  );
  let entrance = $derived(entranceSignRect(layout.columns));
  /** Room for the register's heading, its rows and its "+n more" line. */
  let registerHeight = $derived(REGISTER.head + (REGISTER.rows + 1) * REGISTER.rowH + 12);
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
        more={room.kind === "decision" ? 0 : room.plaque.more}
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
  {#if rows.length > 0}
    <foreignObject x={REGISTER.x} y={REGISTER.y} width={REGISTER.w} height={registerHeight} data-register>
      <Register {rows} />
    </foreignObject>
  {/if}
</svg>
