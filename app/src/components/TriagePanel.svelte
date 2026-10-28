<script lang="ts">
  // The Triage panel (FR-9, AD-55, D-61), at the head of the Subjects view:
  // every row that needs a verdict. Arrows move the focus between rows, `f`
  // and `t` set the focused row's verdict, `Enter` confirms that row only;
  // a held key repeats nothing. Confirm all shown takes a pointer click only
  // (D-28) and leaves out, counted apart, the rows without a verdict, those
  // without a subject, those being confirmed and those the model proposed
  // `finished`, which are confirmed one by one. The evidence on the triage
  // model closes the panel.
  import { tick } from "svelte";
  import type { TriageRow as Row } from "../lib/generated/TriageRow";
  import type { LeftOut, RegistryStore } from "../lib/registry.svelte";
  import { isPointerClick } from "../lib/pointer";
  import TriageEvidence from "./TriageEvidence.svelte";
  import TriageRow from "./TriageRow.svelte";

  interface Props {
    store: RegistryStore;
  }
  let { store }: Props = $props();

  let rows = $derived(store.rows);
  let panel: HTMLElement | undefined = $state();
  /** The row holding the keyboard focus, and its place in the list. */
  let focusKey = $state<string | null>(null);
  let focusIndex = $state(0);
  /** True while the focus is on a row, so a row leaving the list hands it
   *  to the next one instead of dropping it on the page. */
  let focusInPanel = $state(false);

  let taken = $derived(rows.filter((row) => store.leftOut(row) === null));
  let leftOut = $derived.by(() => {
    const counts: Record<LeftOut, number> = { unsure: 0, no_subject: 0, in_flight: 0, model_finished: 0 };
    for (const row of rows) {
      const why = store.leftOut(row);
      if (why !== null) counts[why] += 1;
    }
    return counts;
  });
  let leftOutText = $derived(
    [
      leftOut.unsure > 0 ? `${leftOut.unsure} without a verdict` : null,
      leftOut.no_subject > 0 ? `${leftOut.no_subject} without a subject` : null,
      leftOut.in_flight > 0 ? `${leftOut.in_flight} being confirmed` : null,
      leftOut.model_finished > 0 ? `${leftOut.model_finished} the model calls finished, to confirm one by one` : null,
    ]
      .filter((part) => part !== null)
      .join(", "),
  );

  function element(key: string): HTMLElement | null {
    return panel?.querySelector<HTMLElement>(`[data-row="${CSS.escape(key)}"]`) ?? null;
  }

  async function focusRow(index: number): Promise<void> {
    const row = rows[index];
    if (!row) return;
    focusKey = row.key;
    focusIndex = index;
    await tick();
    element(row.key)?.focus();
  }

  // When the row in focus leaves the list (a view dropped it after its
  // confirmation), the focus goes to the row taking its place.
  $effect(() => {
    if (focusKey === null || rows.some((row) => row.key === focusKey)) return;
    if (rows.length === 0) {
      focusKey = null;
      return;
    }
    const next = Math.min(focusIndex, rows.length - 1);
    if (focusInPanel) void focusRow(next);
    else focusKey = rows[next]?.key ?? null;
  });

  // The header's "N to sort" chip opens the panel on its first row.
  $effect(() => {
    if (!store.openPanel) return;
    store.openPanel = false;
    panel?.scrollIntoView({ block: "start" });
    void focusRow(0);
  });

  function onKey(event: KeyboardEvent, row: Row, index: number): void {
    if (event.ctrlKey || event.altKey || event.metaKey) return;
    const target = event.target as HTMLElement;
    if (["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
    // The keys the panel takes, each with what it does: one map, so a
    // key is handled exactly when it has an action.
    const actions: Record<string, () => void> = {
      ArrowDown: () => void focusRow(Math.min(index + 1, rows.length - 1)),
      ArrowUp: () => void focusRow(Math.max(index - 1, 0)),
      f: () => store.setVerdict(row.key, "finished"),
      t: () => store.setVerdict(row.key, "to_resume"),
      Enter: () => {
        if (!store.confirming[row.key]) void store.confirm([row]);
      },
    };
    const action = Object.hasOwn(actions, event.key) ? actions[event.key] : undefined;
    if (action === undefined) return;
    // Enter on a button inside the row is that button's own.
    if (event.key === "Enter" && target !== event.currentTarget) return;
    event.preventDefault();
    if (event.repeat) return;
    action();
  }

  function confirmAll(event: MouseEvent): void {
    if (!isPointerClick(event)) return;
    void store.confirm(taken);
  }

  /** Enter and Space never confirm all (D-28): the click they would
   *  synthesise is stopped before it is made. */
  function noKeyConfirms(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") event.preventDefault();
  }

  function focusLeft(event: FocusEvent): void {
    const next = event.relatedTarget as Node | null;
    // A row removed from the page takes the focus with it, to nowhere:
    // that is not the author leaving the panel.
    if (next !== null && !panel?.contains(next)) focusInPanel = false;
  }
</script>

<section
  class="panel"
  data-testid="triage-panel"
  aria-labelledby="triage-title"
  bind:this={panel}
  onfocusin={() => (focusInPanel = true)}
  onfocusout={focusLeft}
>
  <div class="head">
    <h2 id="triage-title">To sort</h2>
    {#if rows.length > 0}
      <button
        class="tool"
        data-testid="confirm-all"
        disabled={taken.length === 0}
        onclick={confirmAll}
        onkeydown={noKeyConfirms}
      >
        Confirm all {taken.length} shown{#if leftOutText}<span class="left-out" data-testid="confirm-all-left-out"
            >; left out: {leftOutText}</span
          >{/if}
      </button>
    {/if}
  </div>
  {#if rows.length === 0}
    <p class="muted" data-testid="triage-none">Nothing to sort: every finished subject has its verdict.</p>
  {:else}
    <p class="muted keys">Arrows move between rows, f and t set the verdict, Enter confirms the row in focus.</p>
  {/if}
  {#if store.triageNote}
    <p class="note" role="status" data-testid="triage-note">{store.triageNote}</p>
  {/if}
  <ol class="rows">
    {#each rows as row, index (row.key)}
      <!-- Each row takes the focus and the panel's keys (AD-55), as an
           article of the WAI feed pattern does; its controls stay buttons. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <li
        data-row={row.key}
        tabindex={row.key === focusKey || (focusKey === null && index === 0) ? 0 : -1}
        onfocus={() => {
          focusKey = row.key;
          focusIndex = index;
        }}
        onkeydown={(event) => onKey(event, row, index)}
      >
        <TriageRow {row} {store} />
      </li>
    {/each}
  </ol>
  <TriageEvidence {store} />
</section>

<style>
  .panel {
    padding: 12px 18px;
    border-radius: 16px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .muted {
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .note {
    font-size: 13.5px;
  }
  .rows {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 12px;
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }
  li {
    border-radius: 16px;
  }
  li:focus-visible,
  li:focus {
    outline: 3px solid var(--color-text);
    outline-offset: 2px;
  }
  .tool {
    min-height: 36px;
    padding: 0 14px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .left-out {
    font-weight: 400;
  }
</style>
