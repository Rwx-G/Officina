<script lang="ts">
  // The Sessions view's modes (AD-66): a radio group, one tab stop, arrows
  // move the choice as the ARIA pattern says. A fixed control: its labels
  // never change, so choosing a mode moves nothing beside it (D-517).
  import type { Base } from "../lib/view.svelte";

  interface Props {
    base: Base;
    onBase: (base: Base) => void;
    /** Kept in the header outside the Sessions view, so switching views
     *  slides nothing, but neither seen nor reachable there. */
    shown: boolean;
  }
  let { base, onBase, shown }: Props = $props();

  const MODES: { id: Base; label: string }[] = [
    { id: "board", label: "Board" },
    { id: "scene", label: "Scene" },
  ];

  let group: HTMLDivElement;

  function onKey(event: KeyboardEvent): void {
    const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key];
    if (step === undefined) return;
    event.preventDefault();
    const index = MODES.findIndex((mode) => mode.id === base);
    const next = MODES[(index + step + MODES.length) % MODES.length]!;
    onBase(next.id);
    group.querySelector<HTMLButtonElement>(`[data-mode="${next.id}"]`)?.focus();
  }
</script>

<div
  class="modes"
  class:unseen={!shown}
  role="radiogroup"
  aria-label="Mode"
  data-testid="mode-switch"
  aria-hidden={shown ? undefined : "true"}
  inert={!shown}
  bind:this={group}
>
  {#each MODES as mode (mode.id)}
    <button
      class="mode"
      role="radio"
      data-mode={mode.id}
      data-testid="mode-{mode.id}"
      aria-checked={base === mode.id}
      tabindex={base === mode.id ? 0 : -1}
      title="Switch between Board and Scene (m)"
      onclick={() => onBase(mode.id)}
      onkeydown={onKey}
    >
      {mode.label}
    </button>
  {/each}
</div>

<style>
  .modes {
    display: flex;
    border: 1px solid var(--color-text);
    border-radius: var(--radius-pill);
    overflow: hidden;
  }
  .unseen {
    visibility: hidden;
  }
  .mode {
    min-height: var(--target-min);
    padding: 0 16px;
    border: none;
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .mode[aria-checked="true"] {
    background: var(--color-text);
    color: var(--color-bg);
  }
</style>
