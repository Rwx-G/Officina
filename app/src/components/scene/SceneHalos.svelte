<script lang="ts">
  // Layer 2 (AD-59, AD-65): one halo per employee standing at a place of the
  // decision office or the alert room, and the alert room's beacon glow
  // while the room is lit (AD-175), HTML so their `opacity` animation is
  // composited. Each is timed by story 4.4's `breathe`, which shares one
  // memo with the board's ring: a mode switch or a remount never restarts
  // the breathing (D-108, D-522). A halo is keyed by card key and the start
  // of the wait, the beacon by the first alerted card (D-474). A figure on
  // its way has no halo; it gets it on arrival, under the same key, so the
  // memo keeps its phase across a move between the two rooms (AD-65).
  import type { Card } from "../../lib/generated/Card";
  import { breathe, haloEnteredAt, type HaloPair } from "../../lib/halo";
  import { BEACON, beaconPoint } from "../../lib/scene/geometry";
  import { paint } from "../../lib/scene/furniture";
  import { alertLit, type SceneLayout } from "../../lib/scene/layout";

  interface Props {
    layout: SceneLayout;
    cards: ReadonlyMap<string, Card>;
    /** What the beacon breathes for: `beaconPair` of the board. */
    beacon: HaloPair | null;
    /** The keys walking or gliding to their place (the mover's). */
    transit?: ReadonlySet<string>;
  }
  let { layout, cards, beacon, transit = new Set() }: Props = $props();

  /** The prototype's ellipse for a standing figure, centred above its
   *  floor point, in logical px. */
  const HALO = { rx: 27, ry: 38, lift: 36 } as const;

  let halos = $derived(
    layout.employees.flatMap((e) => {
      if ((e.zone !== "decision" && e.zone !== "alert") || e.at === null || transit.has(e.key)) return [];
      const card = cards.get(e.key);
      const enteredAt = card ? haloEnteredAt(card) : null;
      // In the alert room the halo takes the alert colour (AD-65 as amended).
      const colour = paint(e.zone === "alert" ? "color-alert" : "color-attention");
      return [{ key: e.key, enteredAt, colour, x: e.at.x - HALO.rx, y: e.at.y - HALO.lift - HALO.ry }];
    }),
  );

  let lit = $derived(alertLit(layout));
  let lamp = $derived(beaconPoint(layout.columns));
</script>

<div class="layer" aria-hidden="true" style:pointer-events="none" data-layer="halos">
  {#each halos as halo (`${halo.key}:${halo.enteredAt}`)}
    <div
      class="halo"
      data-halo={halo.key}
      style:--halo-colour={halo.colour}
      style:left="{halo.x}px"
      style:top="{halo.y}px"
      style:width="{2 * HALO.rx}px"
      style:height="{2 * HALO.ry}px"
      use:breathe={{ key: halo.key, enteredAt: halo.enteredAt }}
    ></div>
  {/each}
  {#if lit}
    <div
      class="beacon"
      data-beacon={beacon?.key ?? ""}
      style:left="{lamp.x - BEACON.glow.rx}px"
      style:top="{lamp.y - BEACON.glow.ry}px"
      style:width="{2 * BEACON.glow.rx}px"
      style:height="{2 * BEACON.glow.ry}px"
      use:breathe={beacon}
    ></div>
  {/if}
</div>

<style>
  .halo,
  .beacon {
    position: absolute;
    border-radius: 50%;
    opacity: var(--halo-rest);
    will-change: opacity;
    animation: halo-breathe var(--halo-half) ease-in-out var(--halo-delay) var(--halo-count) alternate;
  }
  .halo {
    background: radial-gradient(closest-side, var(--halo-colour) 45%, transparent);
  }
  /* The prototype v2's beacon glow: the alert colour fading out from the
     lamp, resting at its lit opacity. */
  .beacon {
    --halo-rest: 0.55;
    background: radial-gradient(closest-side, color-mix(in srgb, var(--color-alert) 90%, transparent), transparent);
  }
</style>
