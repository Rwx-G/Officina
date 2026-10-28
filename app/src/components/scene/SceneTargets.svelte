<script lang="ts">
  // Layer 5 (AD-59, AD-63): one transparent button per layout target, at its
  // rectangle in logical px under the scene's transform, in the layout's tab
  // order. It carries every name, role and focus of the scene. The name tag
  // is in the button from the start and shown by CSS on hover and keyboard
  // focus, so neither writes anything (AD-63). Arrows move within the
  // focused target's group (D-187) and never scroll the sessions area.
  import { nearestTarget, type Direction, type SceneLayout, type Target } from "../../lib/scene/layout";

  interface Props {
    layout: SceneLayout;
    onActivate: (target: Target) => void;
  }
  let { layout, onActivate }: Props = $props();

  const ARROWS: Record<string, Direction> = {
    ArrowUp: "up",
    ArrowDown: "down",
    ArrowLeft: "left",
    ArrowRight: "right",
  };

  let layer: HTMLElement;

  /** Focuses the target `id`; false when the layout has none. */
  export function focusTarget(id: string): boolean {
    const button = Array.from(layer.querySelectorAll<HTMLButtonElement>("[data-target]")).find((b) => b.dataset.target === id);
    button?.focus();
    return button !== undefined;
  }

  function onKey(event: KeyboardEvent, target: Target): void {
    const direction = ARROWS[event.key];
    if (!direction || event.ctrlKey || event.altKey || event.metaKey || event.shiftKey) return;
    // At a group's edge focus stays put, and the area does not scroll.
    event.preventDefault();
    const next = nearestTarget(layout.targets, target.id, direction);
    if (next) focusTarget(next);
  }
</script>

<div class="targets" data-layer="targets" bind:this={layer}>
  {#each layout.targets as target (target.id)}
    <button
      type="button"
      class="target {target.kind}"
      data-target={target.id}
      data-key={target.kind === "figure" ? target.keys[0] : undefined}
      aria-label={target.label}
      style:left="{target.rect.x}px"
      style:top="{target.rect.y}px"
      style:width="{target.rect.w}px"
      style:height="{target.rect.h}px"
      onclick={() => onActivate(target)}
      onkeydown={(event) => onKey(event, target)}
    >
      <span class="tag" aria-hidden="true" data-tag style:pointer-events="none">{target.tag}</span>
    </button>
  {/each}
</div>

<style>
  .targets {
    position: absolute;
    inset: 0;
  }
  .target {
    position: absolute;
    margin: 0;
    padding: 0;
    border: 0;
    border-radius: 12px;
    background: transparent;
    cursor: pointer;
  }
  /* Two tones (D-188): one of the two contrasts with whatever the ring
     crosses. */
  .target:focus-visible {
    outline: 3px solid var(--color-working);
    outline-offset: 2px;
    box-shadow: 0 0 0 2px var(--color-surface);
  }
  .tag {
    position: absolute;
    left: 50%;
    bottom: calc(100% + 6px);
    transform: translateX(-50%);
    max-width: 180px;
    padding: 3px 10px;
    border-radius: var(--radius-pill);
    background: var(--color-text);
    color: var(--color-bg);
    font-size: 12px;
    font-weight: 700;
    line-height: 1.3;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    visibility: hidden;
  }
  .target:hover .tag,
  .target:focus-visible .tag {
    visibility: visible;
  }
</style>
