<script lang="ts">
  // The Walking toggle (AD-66 as amended, D-469): figures walk to their
  // places, or glide there when it is off. A fixed control: its label never
  // changes, so pressing it moves nothing beside it (D-517). Shown and
  // usable in every view and mode, like every header button (D-540): the
  // setting is the scene's, and takes effect there.
  interface Props {
    walking: boolean;
    onWalking: (walking: boolean) => void;
  }
  let { walking, onWalking }: Props = $props();
</script>

<button
  class="walking"
  data-testid="walking-toggle"
  aria-pressed={walking}
  title="In the scene, let the figures walk to their places, or move them straight there"
  onclick={() => onWalking(!walking)}
>
  Walking
</button>

<style>
  .walking {
    min-height: var(--target-min);
    padding: 0 12px;
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
</style>
