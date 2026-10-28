<script lang="ts">
  // Layer 2 (AD-59, AD-65): one halo per employee standing at a place of the
  // decision office or the alert room, HTML so its `opacity` animation is
  // composited. Keyed by card key and the start of the wait, and timed by
  // story 4.4's `breathe`, which shares one memo with the board's ring: a
  // mode switch or a remount never restarts the breathing (D-108, D-522).
  import type { Card } from "../../lib/generated/Card";
  import { breathe } from "../../lib/halo";
  import type { SceneLayout } from "../../lib/scene/layout";

  interface Props {
    layout: SceneLayout;
    cards: ReadonlyMap<string, Card>;
  }
  let { layout, cards }: Props = $props();

  /** The prototype's ellipse for a standing figure, centred above its
   *  floor point, in logical px. */
  const HALO = { rx: 27, ry: 38, lift: 36 } as const;

  let halos = $derived(
    layout.employees.flatMap((e) => {
      if ((e.zone !== "decision" && e.zone !== "alert") || e.at === null) return [];
      const card = cards.get(e.key);
      // When the wait began, as the board's ring counts it (lib/halo.ts).
      const enteredAt = card ? (card.last_event_at_ms ?? card.started_at_ms) : null;
      return [{ key: e.key, enteredAt, x: e.at.x - HALO.rx, y: e.at.y - HALO.lift - HALO.ry }];
    }),
  );
</script>

<div class="layer" aria-hidden="true" style:pointer-events="none" data-layer="halos">
  {#each halos as halo (`${halo.key}:${halo.enteredAt}`)}
    <div
      class="halo"
      data-halo={halo.key}
      style:left="{halo.x}px"
      style:top="{halo.y}px"
      style:width="{2 * HALO.rx}px"
      style:height="{2 * HALO.ry}px"
      use:breathe={{ key: halo.key, enteredAt: halo.enteredAt }}
    ></div>
  {/each}
</div>

<style>
  .halo {
    position: absolute;
    border-radius: 50%;
    background: radial-gradient(closest-side, var(--color-attention) 45%, transparent);
    opacity: var(--halo-rest);
    will-change: opacity;
    animation: halo-breathe var(--halo-half) ease-in-out var(--halo-delay) var(--halo-count) alternate;
  }
</style>
