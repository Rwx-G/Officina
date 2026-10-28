<script lang="ts">
  // An ended card's menu (story 4.15, D-542): what dragging it does, for
  // the keyboard and for anyone who prefers a menu. The menu button
  // pattern: Enter, Space or Down opens it on its first item, Up on its
  // last; arrows, Home and End move; Escape closes it and gives the focus
  // back to its button; Tab leaves it closed.
  import { tick } from "svelte";
  import type { Card } from "../lib/generated/Card";
  import { fadeMotion } from "../lib/motion";
  import { TARGET_TEXT, targetsFor, type SettleTarget } from "../lib/settle";

  interface Props {
    card: Card;
    busy: boolean;
    onChoose: (target: SettleTarget) => void;
  }
  let { card, busy, onChoose }: Props = $props();

  let open = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLElement | null>(null);
  let targets = $derived(targetsFor(card));

  function items(): HTMLButtonElement[] {
    return Array.from(menu?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? []);
  }

  async function show(at: "first" | "last"): Promise<void> {
    open = true;
    await tick();
    const all = items();
    (at === "first" ? all[0] : all.at(-1))?.focus();
  }

  function close(focusBack: boolean): void {
    open = false;
    if (focusBack) trigger?.focus();
  }

  function onTriggerKey(event: KeyboardEvent): void {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      void show(event.key === "ArrowDown" ? "first" : "last");
    }
  }

  function onMenuKey(event: KeyboardEvent): void {
    const all = items();
    const at = all.indexOf(document.activeElement as HTMLButtonElement);
    const move = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: all.length - 1 }[event.key];
    if (move !== undefined) {
      event.preventDefault();
      all[(move + all.length) % all.length]?.focus();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close(true);
    } else if (event.key === "Tab") {
      close(false);
    }
  }

  function choose(target: SettleTarget): void {
    open = false;
    onChoose(target);
  }

  /** Closes the menu when the focus leaves it for anything but its button. */
  function onFocusOut(event: FocusEvent): void {
    const next = event.relatedTarget as Node | null;
    if (open && !(next && (menu?.contains(next) || trigger?.contains(next)))) open = false;
  }
</script>

{#if targets.length > 0}
  <div class="card-menu" data-no-drag>
    <button
      bind:this={trigger}
      type="button"
      class="trigger"
      data-testid="card-menu"
      aria-haspopup="menu"
      aria-expanded={open}
      aria-label="Actions for {card.name}"
      title="Mark done, back to To resume, or discard"
      disabled={busy}
      onclick={() => (open ? close(true) : void show("first"))}
      onkeydown={onTriggerKey}
    >
      <span aria-hidden="true">&#8943;</span>
    </button>
    {#if open}
      <div
        class="menu"
        role="menu"
        tabindex="-1"
        aria-label="Actions for {card.name}"
        data-testid="card-menu-list"
        bind:this={menu}
        onkeydown={onMenuKey}
        onfocusout={onFocusOut}
        transition:fadeMotion
      >
        {#each targets as target (target)}
          <button type="button" role="menuitem" tabindex="-1" data-testid="card-menu-{target}" onclick={() => choose(target)}>
            {TARGET_TEXT[target]}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .card-menu {
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 2;
  }
  .trigger {
    width: 32px;
    height: 32px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--color-text-muted);
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
  }
  .trigger:hover,
  .trigger[aria-expanded="true"] {
    border-color: var(--color-border);
    background: var(--color-bg);
    color: var(--color-text);
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    display: flex;
    flex-direction: column;
    min-width: 180px;
    padding: 4px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-surface);
    background: var(--color-surface);
    box-shadow: 0 6px 18px var(--color-shadow);
  }
  [role="menuitem"] {
    min-height: 36px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    text-align: left;
    font-weight: 600;
    cursor: pointer;
  }
  [role="menuitem"]:hover,
  [role="menuitem"]:focus-visible {
    background: var(--color-bg);
  }
</style>
