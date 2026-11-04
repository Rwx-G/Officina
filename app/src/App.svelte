<script lang="ts">
  import { onMount, tick } from "svelte";
  import { BoardStore } from "./lib/board.svelte";
  import { RegistryStore } from "./lib/registry.svelte";
  import { RelayStore } from "./lib/relay.svelte";
  import { OrchestratorStore } from "./lib/orchestrator.svelte";
  import { ViewStore, keyAction, type WindowView } from "./lib/view.svelte";
  import { startAll } from "./lib/lifecycle";
  import { REGISTRY_STATE_TEXT } from "./lib/registry-text";
  import { clock, hours } from "./lib/time";
  import { fadeMotion } from "./lib/motion";
  import Header from "./components/Header.svelte";
  import AlertBand from "./components/AlertBand.svelte";
  import DiscardTarget from "./components/DiscardTarget.svelte";
  import SettleNote from "./components/SettleNote.svelte";
  import BoardColumn from "./components/BoardColumn.svelte";
  import FocusList from "./components/FocusList.svelte";
  import RelayBanners from "./components/RelayBanners.svelte";
  import OrchestratorView from "./components/OrchestratorView.svelte";
  import RelayView from "./components/RelayView.svelte";
  import SubjectsView from "./components/SubjectsView.svelte";
  import Scene from "./components/scene/Scene.svelte";
  import MeetingPanel from "./components/MeetingPanel.svelte";
  import { meeting } from "./lib/meeting.svelte";
  import { workshop } from "./lib/workshop.svelte";
  import ReviewPanel from "./components/ReviewPanel.svelte";
  import type { MeetingRun } from "./lib/generated/MeetingRun";

  const store = new BoardStore();
  const relay = new RelayStore();
  const registry = new RegistryStore();
  const orchestrator = new OrchestratorStore();
  const windowView = new ViewStore();
  const DONE_VISIBLE = 5;

  // A meeting is held in the Sessions view (AD-93): Receive from the
  // Subjects view shows it there. It takes the review panel's slot, one
  // panel at a time (AD-84, AD-122).
  meeting.onOpen = () => {
    workshop.close();
    windowView.view = "sessions";
  };
  // The open meeting asks again when the board shows its card changed.
  $effect(() => {
    if (store.board) meeting.boardChanged(store.board);
  });

  /** A failed run's journal entry in the Relay view (AD-93): focused when
   *  the Relay view's recent entries still list it. */
  async function showJournal(run: MeetingRun): Promise<void> {
    windowView.view = "relay";
    await tick();
    const row = run.run_id ? document.querySelector<HTMLElement>(`[data-run-id="${CSS.escape(run.run_id)}"]`) : null;
    row?.scrollIntoView({ block: "center" });
    row?.focus();
  }

  /** To resume or Done from the header (D-541): Board mode on that column,
   *  its heading focused and brought into view. */
  async function openEnded(column: "to_resume" | "done"): Promise<void> {
    windowView.view = "sessions";
    windowView.setBase("board");
    await tick();
    const heading = document.getElementById(`title-${column}`);
    heading?.focus();
    heading?.scrollIntoView({ block: "nearest" });
  }

  /** After an undo, the focus goes back to the card where the board now
   *  places it: on the board shown, or on the next one, which the
   *  collector may publish after the answer; failing that, it stays on To
   *  resume's heading. */
  let refocus = $state<{ key: string; board: unknown } | null>(null);
  function undone(key: string): void {
    document.getElementById("title-to_resume")?.focus();
    refocus = { key, board: store.board };
  }
  $effect(() => {
    if (refocus === null || !store.board) return;
    const { key, board } = refocus;
    const last = store.board !== board;
    void tick().then(() => {
      // A card on its way out is inert while it fades: never that one.
      const card = Array.from(
        document.querySelectorAll<HTMLElement>(`[data-testid="card"][data-key="${CSS.escape(key)}"] button.main`),
      ).find((button) => !button.closest("[inert]"));
      card?.focus();
      if (card || last) refocus = null;
    });
  });

  // A workstation without the relay has no Relay view to show.
  let view = $derived<WindowView>(windowView.view === "relay" && !relay.inUse ? "sessions" : windowView.view);
  let scene = $derived(view === "sessions" && windowView.mode === "scene");

  // Leaving the Orchestrator view, by any path, is a reading of its
  // briefing (AD-83, AD-93); closing the window is the backend's.
  let shownView: WindowView = "sessions";
  $effect(() => {
    const current = view;
    if (shownView === "orchestrator" && current !== "orchestrator") void orchestrator.markRead();
    shownView = current;
  });

  onMount(() =>
    startAll(
      () => store.start(),
      () => relay.start(),
      () => registry.start(),
      () => orchestrator.start(),
      () => workshop.start(),
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
    // The Subjects view's triage keeps its own `f` (AD-55, AD-66).
    const action = keyAction(view, event.key);
    if (action === null) return;
    // A held key switches once, not once per auto-repeat.
    switch (action.kind) {
      case "refresh":
        void store.refresh();
        return;
      case "toggle_base":
        if (!event.repeat) windowView.toggleBase();
        return;
      case "toggle_focus":
        if (!event.repeat) windowView.toggleFocus();
        return;
      case "focus_needs_you": {
        const card = store.needsYou[action.index];
        if (card) void store.focus(card);
        return;
      }
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
    {orchestrator}
    {view}
    onView={(next) => (windowView.view = next)}
    base={windowView.base}
    focus={windowView.focus}
    onMode={(next) => windowView.setMode(next)}
    theme={windowView.theme}
    resolvedTheme={windowView.resolvedTheme}
    onTheme={() => windowView.cycleTheme()}
    walking={windowView.walking}
    onWalking={(next) => windowView.setWalking(next)}
    onEnded={(column) => void openEnded(column)}
  />
  <AlertBand {board} nowMs={store.nowMs} />

  {#if view === "relay"}
    <RelayView store={relay} nowMs={store.nowMs} />
  {:else if view === "orchestrator"}
    <OrchestratorView store={orchestrator} {board} nowMs={store.nowMs} />
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
    {:else if windowView.mode === "focus"}
      <FocusList {board} {store} onApprovals={() => windowView.setMode("board")} base={windowView.base} />
    {:else if scene}
      <main class="sessions-area" data-testid="sessions-area" in:fadeMotion>
        <Scene
          {board}
          {store}
          onBoard={() => windowView.setBase("board")}
          walking={windowView.walking}
          {orchestrator}
          onOrchestrator={() => (windowView.view = "orchestrator")}
          relay={relay.relay}
        />
      </main>
    {:else if columns}
      <main class="board" in:fadeMotion>
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
          hint="Finished in the last {doneWindow}: background sessions done, and those you marked done or whose subject you archived."
          cards={columns.done}
          {store}
          collapseAfter={DONE_VISIBLE}
        />
      </main>
    {/if}
  {/if}
  {#if meeting.key !== null && view === "sessions"}
    <MeetingPanel {board} {store} onJournal={(run) => void showJournal(run)} />
  {/if}
  {#if workshop.runId !== null && view === "sessions"}
    <ReviewPanel nowMs={store.nowMs} />
  {/if}
  <DiscardTarget />
  <SettleNote {store} onUndone={undone} />
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
  /* Five columns side by side down to the window's minimum width (D-546). */
  .board {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
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
    border-radius: var(--radius-surface);
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
