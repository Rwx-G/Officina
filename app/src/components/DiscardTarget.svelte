<script lang="ts">
  // Where a dragged card goes to be discarded (story 4.15, D-536): shown
  // only while a card that can be discarded is dragged, over the window's
  // bottom edge, so its coming and going moves nothing (D-517).
  import { DROP_ATTRIBUTE, drag } from "../lib/drag.svelte";
  import { fadeMotion } from "../lib/motion";
</script>

{#if drag.takes("discard")}
  <div
    class="discard"
    class:over={drag.over === "discard"}
    {...{ [DROP_ATTRIBUTE]: "discard" }}
    data-testid="discard-target"
    transition:fadeMotion
  >
    Discard: never real work
  </div>
{/if}

<style>
  .discard {
    position: fixed;
    left: 50%;
    bottom: 20px;
    z-index: 15;
    display: flex;
    align-items: center;
    justify-content: center;
    width: min(420px, calc(100vw - 40px));
    min-height: 64px;
    translate: -50% 0;
    border: 2px dashed var(--color-danger);
    border-radius: var(--radius-surface);
    background: var(--color-danger-surface);
    color: var(--color-danger);
    font-weight: 700;
  }
  .over {
    border-style: solid;
  }
</style>
