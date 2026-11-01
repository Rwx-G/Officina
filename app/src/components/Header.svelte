<script lang="ts">
  import type { BoardStore } from "../lib/board.svelte";
  import type { OrchestratorStore } from "../lib/orchestrator.svelte";
  import type { RegistryStore } from "../lib/registry.svelte";
  import type { RelayStore } from "../lib/relay.svelte";
  import { DEFAULT_WALKING, type Base, type Mode, type WindowView } from "../lib/view.svelte";
  import { RELAY_TOGGLE_TEXT } from "../lib/relay-text";
  import { switchText } from "../lib/orchestrator-text";
  import WorkCounts, { type EndedColumn } from "./WorkCounts.svelte";
  import { fadeMotion } from "../lib/motion";
  import QuotaGauges from "./QuotaGauges.svelte";
  import ModeSwitch from "./ModeSwitch.svelte";
  import StableLabel from "./StableLabel.svelte";
  import ThemeSwitch from "./ThemeSwitch.svelte";
  import WalkingSwitch from "./WalkingSwitch.svelte";
  import type { Theme, ThemeChoice } from "../lib/theme";
  import logo from "../assets/logo.svg";
  import logoDark from "../assets/logo-dark.svg";

  interface Props {
    store: BoardStore;
    relay?: RelayStore;
    /** Its rows to sort, a count of the sessions indicator (D-61, D-1041). */
    registry?: RegistryStore;
    /** Its presence marks the Orchestrator view button (D-565). */
    orchestrator?: OrchestratorStore;
    /** The view shown, local window state (AD-66). */
    view?: WindowView;
    onView?: (view: WindowView) => void;
    /** The Sessions view's base. */
    base?: Base;
    /** Focus over the base (AD-66). */
    focus?: boolean;
    onMode?: (mode: Mode) => void;
    /** The theme control (AD-67); the logo follows the resolved theme. */
    theme?: ThemeChoice;
    resolvedTheme?: Theme;
    onTheme?: () => void;
    /** The Walking toggle (AD-66), the scene's, shown in every view. */
    walking?: boolean;
    onWalking?: (walking: boolean) => void;
    /** To resume and Done open Board mode on their column (D-541, D-1041). */
    onEnded?: (column: EndedColumn) => void;
  }
  let {
    store,
    relay,
    registry,
    orchestrator,
    view = "sessions",
    onView,
    base = "board",
    focus = false,
    onMode,
    theme = "auto",
    resolvedTheme = "light",
    onTheme,
    walking = DEFAULT_WALKING,
    onWalking,
    onEnded,
  }: Props = $props();

  // Triage blocks no session: its count is neutral, never amber, and the
  // rows it counts stay out of the title and the overlay (AD-55).
  let toSort = $derived(registry?.view ? registry.view.to_sort : null);

  function openTriage(): void {
    if (!registry || !onView) return;
    registry.openPanel = true;
    onView("subjects");
  }

  const VIEWS: { id: WindowView; label: string }[] = [
    { id: "sessions", label: "Sessions" },
    { id: "subjects", label: "Subjects" },
    { id: "relay", label: "Relay" },
    { id: "orchestrator", label: "Orchestrator" },
  ];

  // D-565: the Orchestrator button carries the orchestrator's presence as
  // a neutral shape, never the attention colour: a filled dot while an
  // agents entry shows it live (on the board or in the view, whichever
  // came last), a hollow ring while absent (D-678); and while proposals
  // are pending, their count in the button's corner (D-833).
  let orchestratorLive = $derived((store.board?.orchestrator ?? orchestrator?.view?.presence ?? null) !== null);
  let pending = $derived(orchestrator?.view?.proposals.filter((p) => p.status === "pending").length ?? 0);
  let orchestratorName = $derived(switchText(orchestratorLive, pending));

  // The Relay position only where the relay is in use (story 2.9).
  let views = $derived(VIEWS.filter((option) => option.id !== "relay" || relay?.inUse));

  let board = $derived(store.board);
  let waiting = $derived(board?.waiting_count ?? 0);
  let contract = $derived(board?.contract);
</script>

<!-- One header, one layout, in every view and mode (story 4.15, D-540):
     the brand, the switches, the gauges and the buttons keep the same boxes
     whatever is shown below, on one line on the owner's screens (D-525). -->
<header class="bar">
  <div class="brand">
    <img src={resolvedTheme === "dark" ? logoDark : logo} alt="" width="40" height="40" data-testid="header-logo" />
    <div>
      <h1>Officina</h1>
      <div class="floor">{board?.floor ?? "this workstation"}</div>
    </div>
  </div>

  {#if onView}
    <div class="views" role="group" aria-label="View" data-testid="view-switch">
      {#each views as option (option.id)}
        {#if option.id === "orchestrator"}
          <button
            class="view"
            data-testid="view-{option.id}"
            aria-pressed={view === option.id}
            aria-label={orchestratorName}
            title={orchestratorName}
            onclick={() => onView(option.id)}
          >
            {option.label}
            <span
              class="presence"
              class:live={orchestratorLive}
              data-testid="orchestrator-presence"
              data-presence={orchestratorLive ? "live" : "absent"}
              aria-hidden="true"
            ></span>
            {#if pending > 0}
              <span class="pending" data-testid="orchestrator-pending" aria-hidden="true">{pending}</span>
            {/if}
          </button>
        {:else}
          <button
            class="view"
            data-testid="view-{option.id}"
            aria-pressed={view === option.id}
            onclick={() => onView(option.id)}
          >
            {option.label}
          </button>
        {/if}
      {/each}
    </div>
  {/if}

  {#if onMode}
    <ModeSwitch mode={focus ? "focus" : base} {onMode} shown={view === "sessions"} />
  {/if}

  {#if board}
    <div class="gauges">
      <QuotaGauges quota={board.quota} nowMs={store.nowMs} />
    </div>
  {/if}

  <!-- The right side holds the fixed buttons and, before them, the status
       line of chips that come and go with the data. A chip appearing moves
       no button, and nothing on the left of it, gauges included (D-517). -->
  <div class="side">
    <div class="tools">
      {#if board && onEnded}
        <WorkCounts
          {toSort}
          toResume={board.columns.to_resume.length}
          done={board.columns.done.length}
          doneWindowMs={board.done_window_ms}
          onTriage={registry && onView ? openTriage : undefined}
          {onEnded}
        />
      {/if}
      {#if onWalking}
        <WalkingSwitch {walking} {onWalking} />
      {/if}
      {#if relay?.inUse}
        <button
          class="tool relay-toggle"
          data-testid="relay-toggle"
          aria-pressed={relay.stopped}
          disabled={relay.switching}
          onclick={() => relay.setStopped(!relay.stopped)}
          title={relay.stopped
            ? "Let officina run start approved actions again"
            : "Refuse every officina run call until you resume"}
        >
          <StableLabel labels={RELAY_TOGGLE_TEXT} current={relay.stopped ? "stopped" : "running"} />
        </button>
      {/if}
      <button class="tool" onclick={() => store.refresh()} title="Refresh now (r)">Refresh</button>
      <button
        class="tool"
        data-testid="pin-toggle"
        aria-pressed={store.alwaysOnTop}
        onclick={() => store.toggleAlwaysOnTop()}
        title="Keep Officina above other windows"
      >
        <StableLabel
          labels={{ pinned: "Pinned", unpinned: "Pin" }}
          current={store.alwaysOnTop ? "pinned" : "unpinned"}
        />
      </button>
      {#if onTheme}
        <ThemeSwitch {theme} resolved={resolvedTheme} onCycle={onTheme} />
      {/if}
    </div>
    <div class="status" data-testid="header-status">
      {#if waiting > 0}
        <div class="pill" data-testid="waiting-pill" role="status" transition:fadeMotion>{waiting} waiting</div>
      {/if}
      {#if relay?.stopped}
        <div class="chip stopped" data-testid="relay-stopped-chip" role="status" transition:fadeMotion>relay stopped</div>
      {/if}
      {#if relay?.pending}
        <div class="chip pending" data-testid="catalogue-chip" role="status" transition:fadeMotion>catalogue change</div>
      {/if}
      <!-- The header shows exceptions only: a held contract shows nothing,
           so the status slot keeps its room for what waits (D-679). -->
      {#if contract?.held === false}
        <span class="contract broken" data-testid="contract">contract broken: {contract.missing.join(", ")}</span>
      {/if}
      {#if contract && contract.hook_missing.length > 0}
        <span class="contract broken" data-testid="contract-hook" title="Claude Code {contract.claude_version ?? ''}"
          >hook fields missing: {contract.hook_missing.join(", ")}</span
        >
      {/if}
    </div>
  </div>
</header>

<style>
  /* Items keep to the top of their line: a status line that grows taller
     when its chips wrap moves nothing beside it (D-517). */
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 12px 12px;
    padding: 14px 20px 8px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .floor {
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .views {
    display: flex;
    border: 1px solid var(--color-text);
    border-radius: var(--radius-control);
    overflow: hidden;
  }
  /* 8 px sides rather than the tools' 12: the fourth view leaves the
     status slot its room on a Full HD screen (D-679). */
  .view {
    min-height: var(--target-min);
    padding: 0 8px;
    border: none;
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .view[aria-pressed="true"] {
    background: var(--color-text);
    color: var(--color-bg);
  }
  /* D-565: a hollow ring while absent, a filled dot while live; a shape,
     never a colour of its own. Pressed, it takes the button's text colour,
     on which the muted one would not read. */
  .presence {
    display: inline-block;
    width: 8px;
    height: 8px;
    margin-left: 6px;
    border: 1.5px solid var(--color-text-muted);
    border-radius: var(--radius-round);
    vertical-align: middle;
  }
  .presence.live {
    background: var(--color-text-muted);
  }
  .view[aria-pressed="true"] .presence {
    border-color: currentColor;
  }
  .view[aria-pressed="true"] .presence.live {
    background: currentColor;
  }
  /* The pending count above the mark, in the button's top padding: it
     takes no width of the line, whose Full HD fit has a few pixels left
     (D-679, D-833), so it moves nothing whatever its digits. Neutral like
     the mark. */
  .view[data-testid="view-orchestrator"] {
    position: relative;
  }
  .pending {
    position: absolute;
    top: 3px;
    right: 3px;
    color: var(--color-text-muted);
    font-size: 10.5px;
    font-weight: 700;
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }
  .view[aria-pressed="true"] .pending {
    color: currentColor;
  }
  /* A count badge: the one pill of the header (D-539). */
  .pill {
    padding: 4px 14px;
    font-size: 13.5px;
    white-space: nowrap;
    border-radius: var(--radius-pill);
    background: var(--color-attention);
    color: var(--color-attention-text);
    font-weight: 700;
  }
  .chip {
    padding: 4px 10px;
    font-size: 13.5px;
    white-space: nowrap;
    border-radius: var(--radius-control);
    font-weight: 700;
  }
  .chip.pending {
    background: var(--color-attention);
    color: var(--color-attention-text);
  }
  /* Danger rather than the stopped terracotta, on which the label read
     4.15:1 in the light theme (D-529). */
  .chip.stopped {
    background: var(--color-danger);
    color: var(--color-surface);
  }
  .tool.relay-toggle[aria-pressed="true"] {
    border-color: var(--color-stopped);
    background: var(--color-stopped-surface);
    color: var(--color-stopped-text);
  }
  .gauges {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }
  /* Takes the free room of the line and never more: its smallest width is
     its buttons', so it never wraps to a line of its own. The status sits
     before the buttons and takes what the line leaves: its own width
     counts for nothing (`contain`), so a chip wraps inside it rather than
     pushing the buttons to another line. */
  .side {
    display: flex;
    flex: 1 1 0;
    flex-direction: row-reverse;
    align-items: flex-start;
    gap: 8px;
  }
  .tools {
    display: flex;
    flex: none;
    justify-content: flex-end;
    align-items: center;
    gap: 6px;
  }
  .status {
    display: flex;
    flex: 1 1 0;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: 6px 8px;
    min-width: 0;
    min-height: var(--target-min);
    contain: inline-size;
  }
  .tool {
    min-height: var(--target-min);
    padding: 0 12px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .tool[aria-pressed="true"] {
    background: var(--color-text);
    color: var(--color-bg);
  }
  .contract {
    font-size: 12.5px;
    color: var(--color-text-muted);
  }
  .contract.broken {
    color: var(--color-danger);
    font-weight: 600;
  }
</style>
