<script lang="ts">
  import { onMount } from "svelte";
  import { BoardStore } from "./lib/board.svelte";
  import { clock } from "./lib/time";
  import Header from "./components/Header.svelte";
  import BoardColumn from "./components/BoardColumn.svelte";

  const store = new BoardStore();
  const DONE_VISIBLE = 5;

  onMount(() => {
    let stop: (() => void) | undefined;
    store.start().then((s) => (stop = s));
    return () => stop?.();
  });

  let board = $derived(store.board);
  let columns = $derived(board?.columns);
  let diagnostics = $derived(board?.diagnostics);
  let doneWindow = $derived(board ? `${Math.round(board.done_window_ms / 3_600_000)} h` : "");

  function onKey(event: KeyboardEvent): void {
    if (event.ctrlKey || event.altKey || event.metaKey) return;
    const target = event.target as HTMLElement | null;
    if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
    if (event.key === "r") {
      void store.refresh();
      return;
    }
    const n = Number(event.key);
    if (Number.isInteger(n) && n >= 1 && n <= 9) {
      const card = store.needsYou[n - 1];
      if (card) void store.focus(card);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<Header {store} />

{#if board?.stale}
  <div class="banner" role="alert" data-testid="stale-banner">
    Session list not refreshed since {clock(board.stale.since_ms, store.nowMs)}:
    {board.stale.error}. Showing the last known state.
  </div>
{/if}
{#if diagnostics?.floor_error}
  <div class="banner" role="alert" data-testid="floor-error">Floor file ignored: {diagnostics.floor_error}</div>
{/if}
{#if diagnostics && (diagnostics.corrupt_event_lines > 0 || diagnostics.unrecognized_values.length > 0)}
  <div class="note" data-testid="diagnostics">
    {#if diagnostics.corrupt_event_lines > 0}{diagnostics.corrupt_event_lines} unreadable event lines.{/if}
    {#if diagnostics.unrecognized_values.length > 0}
      Unknown values from Claude Code: {diagnostics.unrecognized_values.join(", ")}.
    {/if}
  </div>
{/if}

{#if !board}
  <main class="loading" data-testid="loading">Asking Claude Code for its sessions...</main>
{:else if columns}
  <main class="board">
    <BoardColumn
      column="needs_you"
      title="Needs you"
      hint="A decision blocks the session. Keys 1-9 open them."
      cards={columns.needs_you}
      {store}
      numbered
    />
    <BoardColumn
      column="your_turn"
      title="Your turn"
      hint="The session finished its turn and waits for your next message."
      cards={columns.your_turn}
      {store}
    />
    <BoardColumn column="working" title="Working" hint="Claude is on it." cards={columns.working} {store} />
    <BoardColumn
      column="to_resume"
      title="To resume"
      hint="Ended or interrupted in the last {doneWindow}."
      cards={columns.to_resume}
      {store}
      collapseAfter={DONE_VISIBLE}
    />
    <BoardColumn
      column="done"
      title="Done"
      hint="Background sessions that finished in the last {doneWindow}."
      cards={columns.done}
      {store}
      collapseAfter={DONE_VISIBLE}
    />
  </main>
{/if}

<style>
  .board {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: 14px;
    align-items: start;
    padding: 0 20px 20px;
  }
  .loading {
    padding: 40px 20px;
    color: var(--color-text-muted);
  }
  .banner {
    margin: 0 20px 12px;
    padding: 10px 16px;
    border-radius: 16px;
    background: #f6e3dc;
    border: 1px solid #c0603c;
    color: var(--color-text);
    font-weight: 600;
  }
  .note {
    margin: 0 20px 12px;
    font-size: 13px;
    color: var(--color-text-muted);
  }
</style>
