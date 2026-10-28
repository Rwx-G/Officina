<script lang="ts">
  import { onMount } from "svelte";
  import { BoardStore } from "./lib/board.svelte";
  import { RegistryStore } from "./lib/registry.svelte";
  import { RelayStore } from "./lib/relay.svelte";
  import { ViewStore, type WindowView } from "./lib/view.svelte";
  import { startAll } from "./lib/lifecycle";
  import { REGISTRY_STATE_TEXT } from "./lib/registry-text";
  import { clock, hours } from "./lib/time";
  import Header from "./components/Header.svelte";
  import AlertBand from "./components/AlertBand.svelte";
  import BoardColumn from "./components/BoardColumn.svelte";
  import RelayBanners from "./components/RelayBanners.svelte";
  import RelayView from "./components/RelayView.svelte";
  import SubjectsView from "./components/SubjectsView.svelte";
  import Scene from "./components/scene/Scene.svelte";

  const store = new BoardStore();
  const relay = new RelayStore();
  const registry = new RegistryStore();
  const windowView = new ViewStore();
  const DONE_VISIBLE = 5;

  // A workstation without the relay has no Relay view to show.
  let view = $derived<WindowView>(windowView.view === "relay" && !relay.inUse ? "sessions" : windowView.view);
  let scene = $derived(view === "sessions" && windowView.base === "scene");

  onMount(() =>
    startAll(
      () => store.start(),
      () => relay.start(),
      () => registry.start(),
    ),
  );

  let board = $derived(store.board);
  let columns = $derived(board?.columns);
  let diagnostics = $derived(board?.diagnostics);
  let registryText = $derived(diagnostics?.registry ? REGISTRY_STATE_TEXT[diagnostics.registry.state] : null);
  let doneWindow = $derived(board ? hours(board.done_window_ms) : "");

  function onKey(event: KeyboardEvent): void {
    if (event.ctrlKey || event.altKey || event.metaKey) return;
    const target = event.target as HTMLElement | null;
    if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
    if (event.key === "r") {
      void store.refresh();
      return;
    }
    // `m` and digits act in the Sessions view only (AD-55, AD-66).
    if (view !== "sessions") return;
    if (event.key === "m") {
      // A held key switches once, not once per auto-repeat.
      if (!event.repeat) windowView.toggleBase();
      return;
    }
    // Digits focus Needs you sessions in every mode (AD-55 as amended).
    const n = Number(event.key);
    if (Number.isInteger(n) && n >= 1 && n <= 9) {
      const card = store.needsYou[n - 1];
      if (card) void store.focus(card);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- In Scene mode the window does not scroll: the sessions area takes what
     the header, the band and the banners leave, and scrolls itself when the
     scene is at its smallest scale (AD-61). -->
<div class="app" class:fill={scene}>
  <Header
    {store}
    {relay}
    {registry}
    {view}
    onView={(next) => (windowView.view = next)}
    base={windowView.base}
    onBase={(next) => windowView.setBase(next)}
    theme={windowView.theme}
    resolvedTheme={windowView.resolvedTheme}
    onTheme={() => windowView.cycleTheme()}
    walking={windowView.walking}
    onWalking={(next) => windowView.setWalking(next)}
  />
  <AlertBand {board} nowMs={store.nowMs} />

  {#if view === "relay"}
    <RelayView store={relay} nowMs={store.nowMs} />
  {:else if view === "subjects"}
    <SubjectsView store={registry} nowMs={store.nowMs} />
  {:else}
    {#if board?.stale}
      <div class="banner" role="alert" data-testid="stale-banner">
        Session list not refreshed since {clock(board.stale.since_ms, store.nowMs)}:
        {board.stale.error}. Showing the last known state.
      </div>
    {/if}
    {#if diagnostics?.floor_error}
      <div class="banner" role="alert" data-testid="floor-error">Floor file ignored: {diagnostics.floor_error}</div>
    {/if}
    {#if registryText}
      <div class="banner" role="alert" data-testid="registry-banner">
        {registryText}
        {#if diagnostics?.registry?.detail}({diagnostics.registry.detail}){/if}
        The board keeps working from Claude Code's sessions.
      </div>
    {/if}
    {#if diagnostics && (diagnostics.corrupt_event_lines > 0 || diagnostics.unrecognized_values.length > 0)}
      <div class="note" data-testid="diagnostics">
        {#if diagnostics.corrupt_event_lines > 0}{diagnostics.corrupt_event_lines} unreadable event lines.{/if}
        {#if diagnostics.unrecognized_values.length > 0}
          Unknown values from Claude Code: {diagnostics.unrecognized_values.join(", ")}.
        {/if}
      </div>
    {/if}

    <RelayBanners store={relay} nowMs={store.nowMs} />

    {#if !board}
      <main class="loading" data-testid="loading">Asking Claude Code for its sessions...</main>
    {:else if scene}
      <main class="sessions-area" data-testid="sessions-area">
        <Scene {board} {store} onBoard={() => windowView.setBase("board")} walking={windowView.walking} />
      </main>
    {:else if columns}
      <main class="board">
        <BoardColumn
          column="needs_you"
          title="Needs you"
          hint="A decision blocks the session. Keys 1-9 open the sessions; approvals take a click."
          cards={columns.needs_you}
          approvals={board.approvals}
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
  {/if}
</div>

<style>
  .fill {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .fill > :global(*) {
    flex: none;
  }
  /* What the header, the band and the banners leave, and never squeezed to
     nothing: banners taller than half the window (a pending catalogue's
     diff) scroll the window instead. */
  .fill > .sessions-area {
    flex: 1 1 0;
    min-height: 50vh;
    /* The scene scrolls inside it; the file panel overlays its right edge. */
    position: relative;
    overflow: hidden;
  }
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
    background: var(--color-danger-surface);
    border: 1px solid var(--color-danger);
    color: var(--color-text);
    font-weight: 600;
  }
  .note {
    margin: 0 20px 12px;
    font-size: 13px;
    color: var(--color-text-muted);
  }
</style>
