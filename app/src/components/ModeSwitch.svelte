<script lang="ts">
  // The Sessions view's modes (AD-66): a radio group, one tab stop, arrows
  // move the choice as the ARIA pattern says. A fixed control: its labels
  // never change, so choosing a mode moves nothing beside it (D-517).
  import type { Mode } from "../lib/view.svelte";

  interface Props {
    mode: Mode;
    onMode: (mode: Mode) => void;
    /** Kept in the header outside the Sessions view, so switching views
     *  slides nothing, but neither seen nor reachable there. */
    shown: boolean;
  }
  let { mode, onMode, shown }: Props = $props();

  const MODES: { id: Mode; label: string }[] = [
    { id: "board", label: "Board" },
    { id: "scene", label: "Scene" },
    { id: "focus", label: "Focus" },
  ];

  let group: HTMLDivElement;

  function onKey(event: KeyboardEvent): void {
    const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key];
    if (step === undefined) return;
    event.preventDefault();
    const index = MODES.findIndex((option) => option.id === mode);
    const next = MODES[(index + step + MODES.length) % MODES.length]!;
    onMode(next.id);
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
  {#each MODES as option (option.id)}
    <button
      class="mode"
      role="radio"
      data-mode={option.id}
      data-testid="mode-{option.id}"
      aria-checked={mode === option.id}
      tabindex={mode === option.id ? 0 : -1}
      title="Switch between Board and Scene (m); Focus lists only what waits for you (f)"
      onclick={() => onMode(option.id)}
      onkeydown={onKey}
    >
      {option.label}
    </button>
  {/each}
</div>

<style>
  .modes {
    display: flex;
    border: 1px solid var(--color-text);
    border-radius: var(--radius-control);
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
