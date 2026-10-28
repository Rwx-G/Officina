<script lang="ts">
  import type { BoardStore } from "../lib/board.svelte";
  import type { RegistryStore } from "../lib/registry.svelte";
  import type { RelayStore } from "../lib/relay.svelte";
  import type { Base, WindowView } from "../lib/view.svelte";
  import { RELAY_TOGGLE_TEXT } from "../lib/relay-text";
  import Gauge from "./Gauge.svelte";
  import ModeSwitch from "./ModeSwitch.svelte";
  import StableLabel from "./StableLabel.svelte";
  import ThemeSwitch from "./ThemeSwitch.svelte";
  import type { Theme, ThemeChoice } from "../lib/theme";
  import logo from "../assets/logo.svg";
  import logoDark from "../assets/logo-dark.svg";

  interface Props {
    store: BoardStore;
    relay?: RelayStore;
    /** Its rows to sort make the neutral "N to sort" chip (D-61). */
    registry?: RegistryStore;
    /** The view shown, local window state (AD-66). */
    view?: WindowView;
    onView?: (view: WindowView) => void;
    /** The Sessions view's mode; Scene lays the header out on one line. */
    base?: Base;
    onBase?: (base: Base) => void;
    /** The theme control (AD-67); the logo follows the resolved theme. */
    theme?: ThemeChoice;
    resolvedTheme?: Theme;
    onTheme?: () => void;
  }
  let {
    store,
    relay,
    registry,
    view = "sessions",
    onView,
    base = "board",
    onBase,
    theme = "auto",
    resolvedTheme = "light",
    onTheme,
  }: Props = $props();

  // Triage blocks no session: the chip is neutral, never amber, and the
  // rows it counts stay out of the title and the overlay (AD-55).
  let toSort = $derived(registry?.view?.to_sort ?? 0);

  function openTriage(): void {
    if (!registry || !onView) return;
    registry.openPanel = true;
    onView("subjects");
  }

  const VIEWS: { id: WindowView; label: string }[] = [
    { id: "sessions", label: "Sessions" },
    { id: "subjects", label: "Subjects" },
    { id: "relay", label: "Relay" },
  ];

  // The Relay position only where the relay is in use (story 2.9).
  let views = $derived(VIEWS.filter((option) => option.id !== "relay" || relay?.inUse));

  let board = $derived(store.board);
  let waiting = $derived(board?.waiting_count ?? 0);
  let contract = $derived(board?.contract);
  // Scene mode keeps the sessions area tall enough for eight projects
  // (D-525): one line, whatever view is shown, so switching views or
  // modes leaves the brand, the switches and the buttons where they are.
  let oneLine = $derived(base === "scene");
</script>

<header class="bar" class:one-line={oneLine}>
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
        <button
          class="view"
          data-testid="view-{option.id}"
          aria-pressed={view === option.id}
          onclick={() => onView(option.id)}
        >
          {option.label}
        </button>
      {/each}
    </div>
  {/if}

  {#if onBase}
    <ModeSwitch {base} {onBase} shown={view === "sessions"} />
  {/if}

  {#if board}
    <div class="gauges">
      <Gauge
        label="5 h"
        window={board.quota.five_hour}
        weekly={false}
        writtenAtMs={board.quota.written_at_ms}
        staleAfterMs={board.quota.stale_after_ms}
        nowMs={store.nowMs}
        compact={oneLine}
      />
      <Gauge
        label="Week"
        window={board.quota.seven_day}
        weekly={true}
        writtenAtMs={board.quota.written_at_ms}
        staleAfterMs={board.quota.stale_after_ms}
        nowMs={store.nowMs}
        compact={oneLine}
      />
    </div>
  {/if}

  <!-- The right side holds the fixed buttons and the status line of chips
       that come and go with the data: under the buttons, or beside them on
       one line. A chip appearing moves no button, and nothing on the left
       of it, gauges included (D-517). -->
  <div class="side">
    <div class="tools">
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
        title="Keep the board above other windows"
      >
        <StableLabel
          labels={{ pinned: "Pinned", unpinned: "Pin on top" }}
          current={store.alwaysOnTop ? "pinned" : "unpinned"}
        />
      </button>
      {#if onTheme}
        <ThemeSwitch {theme} resolved={resolvedTheme} onCycle={onTheme} />
      {/if}
    </div>
    <div class="status" data-testid="header-status">
      {#if waiting > 0}
        <div class="pill" data-testid="waiting-pill" role="status">{waiting} waiting</div>
      {/if}
      {#if relay?.stopped}
        <div class="chip stopped" data-testid="relay-stopped-chip" role="status">relay stopped</div>
      {/if}
      {#if relay?.pending}
        <div class="chip pending" data-testid="catalogue-chip" role="status">catalogue change</div>
      {/if}
      {#if toSort > 0 && onView}
        <button class="chip to-sort" data-testid="to-sort-chip" onclick={openTriage} title="Open the Triage panel">
          {toSort} to sort
        </button>
      {/if}
      {#if contract?.held === true && contract.hook_missing.length === 0}
        <span class="contract" data-testid="contract" title="Claude Code {contract.claude_version ?? ''}">contract held</span>
      {:else if contract?.held === false}
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
    gap: 14px 20px;
    padding: 14px 20px;
  }
  /* The same top and the same order as the Board header, so changing mode
     leaves the brand, the switches, the gauges' corner and the buttons in
     place; the tighter bottom is height given back to the scene. */
  .one-line {
    padding-bottom: 8px;
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
    border-radius: var(--radius-pill);
    overflow: hidden;
  }
  .view {
    min-height: var(--target-min);
    padding: 0 16px;
    border: none;
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .view[aria-pressed="true"] {
    background: var(--color-text);
    color: var(--color-bg);
  }
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
    padding: 4px 12px;
    font-size: 13.5px;
    white-space: nowrap;
    border-radius: var(--radius-pill);
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
  .chip.to-sort {
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
    font-weight: 700;
    cursor: pointer;
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
     its buttons', so it never wraps to a line of its own. */
  .side {
    display: flex;
    flex: 1 1 0;
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
  }
  .tools {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
  }
  /* One chip line is kept even when empty, so the header's height does not
     follow the chips either. */
  .status {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: 6px 10px;
    min-height: 30px;
  }
  /* On one line the status sits before the buttons and takes what the line
     leaves: its own width counts for nothing (`contain`), so a chip wraps
     inside it rather than pushing the buttons to another line. */
  .one-line .side {
    flex-direction: row-reverse;
    align-items: flex-start;
    gap: 10px;
  }
  .one-line .tools {
    flex: none;
  }
  .one-line .status {
    flex: 1 1 0;
    min-width: 0;
    min-height: var(--target-min);
    contain: inline-size;
  }
  .tool {
    min-height: var(--target-min);
    padding: 0 18px;
    border-radius: var(--radius-pill);
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
