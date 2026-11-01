<script lang="ts">
  // What the last verdict from the board did, and the way back (story
  // 4.15, D-542). It floats over the window's bottom edge, so it moves
  // nothing on its way in or out (D-517). A verdict given from a card's
  // menu moved that card, and the focus with it: the note then takes the
  // focus, on Undo when there is one. One line at a fixed width from the
  // minimum window up (D-1040): the session's name can be any length, so
  // the text is cut with an ellipsis, whole in its tooltip, and the note's
  // box does not follow what it says.
  import type { BoardStore } from "../lib/board.svelte";
  import { slideMotion } from "../lib/motion";

  interface Props {
    store: BoardStore;
    /** Gives the focus back after an undo, to the card or its column. */
    onUndone: (key: string) => void;
  }
  let { store, onUndone }: Props = $props();

  let note = $derived(store.settleNote);

  function focusFirst(node: HTMLElement): void {
    if (note?.takeFocus) node.querySelector<HTMLButtonElement>("button")?.focus();
  }

  async function undo(): Promise<void> {
    const key = note?.key;
    if (key && (await store.undoSettle())) onUndone(key);
  }
</script>

{#if note}
  {#key note}
    <div class="note" class:failed={note.failed} role="status" data-testid="settle-note" use:focusFirst transition:slideMotion>
      <span class="text" title={note.text} data-testid="settle-note-text">{note.text}</span>
      {#if note.undo}
        <button type="button" class="tool" data-testid="settle-undo" onclick={() => void undo()}>Undo</button>
      {/if}
      <button type="button" class="tool" data-testid="settle-dismiss" onclick={() => store.dismissSettleNote()}>Dismiss</button>
    </div>
  {/key}
{/if}

<style>
  .note {
    position: fixed;
    left: 50%;
    bottom: 20px;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: 10px;
    width: min(var(--note-width), calc(100vw - 40px));
    padding: 8px 8px 8px 16px;
    translate: -50% 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-surface);
    background: var(--color-surface);
    box-shadow: 0 6px 18px var(--color-shadow);
    font-weight: 600;
  }
  .note.failed {
    border-color: var(--color-danger);
    color: var(--color-danger);
  }
  .text {
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .tool {
    flex: none;
    min-height: 36px;
    padding: 0 12px;
    border: 1px solid var(--color-text);
    border-radius: var(--radius-control);
    background: var(--color-surface);
    color: var(--color-text);
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
  }
</style>
