<script lang="ts" generics="K extends string">
  // The label of a control whose text follows its state. Every label it can
  // show sits in the same grid cell, so the control keeps the size of its
  // longest label in every state and toggling it moves nothing beside it.
  // Only the current label is visible and in the accessibility tree: the
  // control's accessible name is that label. Widths in px would not hold,
  // since fonts and display scaling vary.
  interface Props {
    /** The label of each state the control can be in. */
    labels: Record<K, string>;
    current: K;
  }
  let { labels, current }: Props = $props();

  let states = $derived(Object.keys(labels) as K[]);
</script>

<span class="stable-label" data-stable-label>
  {#each states as state (state)}
    <span class="label" class:inactive={state !== current} aria-hidden={state === current ? undefined : "true"}
      >{labels[state]}</span
    >
  {/each}
</span>

<style>
  .stable-label {
    display: inline-grid;
  }
  .label {
    grid-area: 1 / 1;
  }
  .inactive {
    visibility: hidden;
  }
</style>
