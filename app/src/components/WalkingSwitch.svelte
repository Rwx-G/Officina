<script lang="ts">
  // The Walking toggle (AD-66 as amended, D-469): figures walk to their
  // places, or glide there when it is off. A fixed control: its label never
  // changes, so pressing it moves nothing beside it (D-517).
  interface Props {
    walking: boolean;
    onWalking: (walking: boolean) => void;
    /** Kept in the header outside the Sessions view and in Focus, so
     *  switching slides nothing, but neither seen nor reachable there. */
    shown: boolean;
  }
  let { walking, onWalking, shown }: Props = $props();
</script>

<button
  class="walking"
  class:unseen={!shown}
  data-testid="walking-toggle"
  aria-pressed={walking}
  aria-hidden={shown ? undefined : "true"}
  inert={!shown}
  title="Let the figures walk to their places, or move them straight there"
  onclick={() => onWalking(!walking)}
>
  Walking
</button>

<style>
  .walking {
    min-height: var(--target-min);
    padding: 0 18px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .walking[aria-pressed="true"] {
    background: var(--color-text);
    color: var(--color-bg);
  }
  .unseen {
    visibility: hidden;
  }
</style>
