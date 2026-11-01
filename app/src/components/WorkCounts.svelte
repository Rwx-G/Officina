<script lang="ts">
  // One indicator for the counts the header carried as three controls
  // (story 4.20, the owner's revision of D-541, D-1041): To sort, To resume
  // and Done. The button shows the three counts, each behind its icon, at a
  // fixed width whatever they are, so a count that changes moves nothing
  // (D-517); a small panel under it names each count with its action. It
  // opens on hover, on keyboard focus and on a click, which keeps it open;
  // Escape closes it. The panel floats over the page: its coming and going
  // moves nothing either. Neutral like the chip it replaces: none of these
  // counts waits on the author (AD-55).
  import { hours } from "../lib/time";

  export type EndedColumn = "to_resume" | "done";

  interface Props {
    /** Triage rows to sort; `null` while the registry says nothing. */
    toSort: number | null;
    toResume: number;
    done: number;
    /** The board's Done window, worded as the Board's hints word it. */
    doneWindowMs: number;
    /** Opens the Triage panel; absent where the window has no Subjects view. */
    onTriage?: () => void;
    onEnded: (column: EndedColumn) => void;
  }
  let { toSort, toResume, done, doneWindowMs, onTriage, onEnded }: Props = $props();

  /** Past this a column is no list to read: the count reads "99+". */
  const SHOWN_MAX = 99;
  const shown = (n: number | null): string => (n === null ? "--" : n > SHOWN_MAX ? `${SHOWN_MAX}+` : String(n));

  type Row = { id: "to_sort" | EndedColumn; count: number | null; text: string; action: string; run: (() => void) | null };
  let rows = $derived<Row[]>([
    {
      id: "to_sort",
      count: toSort,
      text: toSort === null ? "To sort: unknown" : `${toSort} to sort`,
      action: "Open the Triage panel",
      run: onTriage && toSort !== null ? onTriage : null,
    },
    {
      id: "to_resume",
      count: toResume,
      text: `${toResume} to resume`,
      action: `Ended or interrupted in the last ${hours(doneWindowMs)}: open them in Board mode`,
      run: () => onEnded("to_resume"),
    },
    {
      id: "done",
      count: done,
      text: `${done} done`,
      action: `Done in the last ${hours(doneWindowMs)}: open them in Board mode`,
      run: () => onEnded("done"),
    },
  ]);
  let summary = $derived(rows.map((r) => r.text).join(", "));

  let pinned = $state(false);
  let hovered = $state(false);
  let focused = $state(false);
  /** Escape closes it until the pointer and the focus have left. */
  let dismissed = $state(false);
  let open = $derived(!dismissed && (pinned || hovered || focused));

  let root: HTMLElement;
  let button: HTMLButtonElement;
  const uid = $props.id();
  const panelId = `work-counts-${uid}`;

  function close(): void {
    pinned = false;
    dismissed = true;
  }

  function onClick(): void {
    if (pinned) {
      close();
      return;
    }
    pinned = true;
    dismissed = false;
  }

  function onFocusIn(event: FocusEvent): void {
    // A click focuses the button too; only the keyboard opens it by focus.
    if (event.target === button && !button.matches(":focus-visible")) return;
    focused = true;
  }

  function onFocusOut(event: FocusEvent): void {
    if (root.contains(event.relatedTarget as Node | null)) return;
    focused = false;
    pinned = false;
    if (!hovered) dismissed = false;
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key !== "Escape" || !open) return;
    event.stopPropagation();
    close();
    button.focus();
  }

  function act(row: Row): void {
    close();
    row.run?.();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="counts"
  bind:this={root}
  onpointerenter={() => (hovered = true)}
  onpointerleave={() => {
    hovered = false;
    if (!focused && !pinned) dismissed = false;
  }}
  onfocusin={onFocusIn}
  onfocusout={onFocusOut}
  onkeydown={onKey}
>
  <button
    type="button"
    class="summary"
    bind:this={button}
    data-testid="work-counts"
    data-stable-slot
    aria-label="Sessions: {summary}"
    aria-expanded={open}
    aria-controls={panelId}
    title={summary}
    onclick={onClick}
  >
    {#each rows as row (row.id)}
      <span class="count" data-testid="work-count-{row.id}">
        <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true" class="icon">
          {#if row.id === "to_sort"}
            <path d="M2 9.5h3.2l1 1.8h3.6l1-1.8H14M2 9.5 4 3.5h8l2 6V13H2z" />
          {:else if row.id === "to_resume"}
            <path d="M12.6 5.2A5.2 5.2 0 1 0 13.2 9M12.8 2.2v3.2H9.6" />
          {:else}
            <path d="M3 8.4 6.4 11.6 13 4.6" />
          {/if}
        </svg>
        <span class="number">{shown(row.count)}</span>
      </span>
    {/each}
  </button>
  {#if open}
    <div class="panel" id={panelId} role="group" aria-label="Sessions to sort, to resume and done" data-testid="work-counts-panel">
      <ul>
        {#each rows as row (row.id)}
          <li>
            <button type="button" class="row" data-testid="counts-{row.id}" disabled={row.run === null} title={row.action} onclick={() => act(row)}>
              <span class="text">{row.text}</span>
              <span class="action">{row.action}</span>
            </button>
          </li>
        {/each}
      </ul>
    </div>
  {/if}
</div>

<style>
  .counts {
    position: relative;
  }
  .summary {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    min-height: var(--target-min);
    padding: 0 10px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
  }
  .summary[aria-expanded="true"] {
    border-color: var(--color-text);
  }
  .count {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .icon {
    flex: none;
    fill: none;
    stroke: var(--color-text-muted);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  /* The widest count it shows, "99+": the button's width never follows. */
  .number {
    min-width: 3ch;
    text-align: left;
    font-variant-numeric: tabular-nums;
  }
  /* Its top padding joins it to the button, so the pointer crossing the
     gap does not close it. */
  .panel {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 30;
    padding-top: 6px;
  }
  ul {
    display: grid;
    gap: 2px;
    width: 300px;
    margin: 0;
    padding: 6px;
    list-style: none;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-surface);
    background: var(--color-surface);
    box-shadow: 0 6px 18px var(--color-shadow);
  }
  .row {
    display: grid;
    gap: 1px;
    width: 100%;
    padding: 6px 10px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--color-text);
    text-align: left;
    cursor: pointer;
  }
  .row:hover:not(:disabled) {
    background: var(--color-bg);
  }
  .row:disabled {
    cursor: default;
  }
  .text {
    font-weight: 700;
  }
  .action {
    color: var(--color-text-muted);
    font-size: 12.5px;
  }
</style>
