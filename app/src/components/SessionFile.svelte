<script lang="ts" module>
  import type { Card } from "../lib/generated/Card";

  /** What the file panel shows: one session's file, or a group's list of
   *  its sessions in board order (D-103). */
  export type FileForm = { kind: "file"; card: Card; back: boolean } | { kind: "list"; title: string; cards: Card[]; focusKey: string | null };
</script>

<script lang="ts">
  // The session file (FR-5, AD-69, D-111): a non-modal side panel over the
  // sessions area's right edge, outside the scene element, so opening it
  // resizes nothing and writes nothing in the scene. It reads the live card
  // and acts on it as its board card does, with the same outcome words
  // (`lib/outcome.ts`). Session text renders as text only.
  import { onMount } from "svelte";
  import type { Board } from "../lib/generated/Board";
  import type { BoardStore } from "../lib/board.svelte";
  import { causeText, waitText } from "../lib/alert";
  import { projectColor } from "../lib/colors";
  import { outcomeText, visibleOutcome } from "../lib/outcome";
  import { liveCards, zoneOf, MAX_NUMBER } from "../lib/scene/layout";
  import { COLUMN_WORDS, BADGE_WORDS, SIGN_WORDS, ZONE_WORDS } from "../lib/scene/text";
  import { clock, duration } from "../lib/time";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    form: FileForm;
    board: Board;
    store: BoardStore;
    /** Escape's step back, also the panel's own button. */
    onClose: () => void;
    /** Opens a listed session's file. */
    onOpen: (key: string) => void;
    /** Switches the Sessions view to Board mode, where approvals are
     *  approved on their cards (D-28). */
    onBoard: () => void;
  }
  let { form, board, store, onClose, onOpen, onBoard }: Props = $props();

  const titleId = $props.id();

  /** The board number of a card, 1 to 9, as keys use it (D-107). */
  const numberOf = (card: Card): number | null => {
    const index = board.columns.needs_you.findIndex((c) => c.key === card.key);
    return index >= 0 && index < MAX_NUMBER ? index + 1 : null;
  };
  const live = $derived(new Set(liveCards(board).map((c) => c.key)));
  const stateWords = (card: Card) => [COLUMN_WORDS[card.column], ...(card.badge ? [BADGE_WORDS[card.badge]] : [])].join(", ");

  let card = $derived(form.kind === "file" ? form.card : null);
  let number = $derived(card ? numberOf(card) : null);
  let waiting = $derived(card !== null && (card.column === "needs_you" || card.scene?.approval === true));
  let since = $derived(card?.scene?.waiting_since_ms ?? null);
  let approvals = $derived(card ? board.approvals.filter((a) => a.session_id !== null && a.session_id === card.session_id) : []);
  let shown = $derived(card ? visibleOutcome(store.outcomes, card.key) : undefined);

  let panel: HTMLElement;

  // Bring to front has focus when the file opens; a list focuses the row
  // it was left from, else its first row.
  onMount(() => {
    if (form.kind === "file") {
      panel.querySelector<HTMLElement>("[data-testid=bring-to-front]")?.focus();
      return;
    }
    const rows = [...panel.querySelectorAll<HTMLElement>("[data-row]")];
    const focusKey = form.focusKey;
    (rows.find((row) => row.dataset.row === focusKey) ?? rows[0])?.focus();
  });
</script>

<div class="file" role="dialog" aria-labelledby={titleId} data-testid="session-file" data-form={form.kind} bind:this={panel}>
  <header>
    <h2 id={titleId}>{form.kind === "file" ? form.card.name : form.title}</h2>
    <button type="button" class="close" data-testid="file-close" onclick={onClose}>
      {form.kind === "file" && form.back ? "Back to the list" : "Close"}
    </button>
  </header>

  {#if form.kind === "list"}
    <p class="muted" data-testid="list-count">{form.cards.length} {form.cards.length === 1 ? "session" : "sessions"}, board order</p>
    <ul class="rows">
      {#each form.cards as row (row.key)}
        {@const n = numberOf(row)}
        <li>
          <button type="button" class="row" data-row={row.key} onclick={() => onOpen(row.key)}>
            {#if n !== null}<span class="number">{n}</span>{/if}
            <span class="row-text">
              <span class="row-name">{row.name}</span>
              <span class="muted">{row.project.name}, {stateWords(row)}</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {:else if card}
    <dl>
      <dt>Project</dt>
      <dd data-testid="file-project">
        <span class="dot" style:background={projectColor(card.project.color)}></span>{card.project.name}
        {#if card.project.priority === "high"}<span class="priority">{SIGN_WORDS.highPriority}</span>{/if}
      </dd>
      {#if number !== null}
        <dt>Number</dt>
        <dd data-testid="file-number">{number}</dd>
      {/if}
      {#if live.has(card.key)}
        <dt>Where</dt>
        <dd data-testid="file-zone">{ZONE_WORDS[zoneOf(card)]}</dd>
      {/if}
      <dt>State</dt>
      <dd data-testid="file-state">
        {stateWords(card)}
        {#if waiting}<span class="pill" data-testid="file-waiting-for">{waitText(card)}</span>{/if}
      </dd>
      {#if waiting}
        <dt>Waiting</dt>
        <dd data-testid="file-since">
          {#if since !== null}since {clock(since, store.nowMs)} ({duration(store.nowMs - since)}){:else}since unknown{/if}
        </dd>
      {/if}
      {#if card.scene?.alert}
        <dt>Alert</dt>
        <dd data-testid="file-alert">
          {causeText(card, store.nowMs)}{#if zoneOf(card) === "unknown"}, last known{/if}
        </dd>
      {/if}
      {#if card.subject}
        <dt>Subject</dt>
        <dd data-testid="file-subject">{card.subject.name}</dd>
      {/if}
    </dl>

    {#each approvals as approval (approval.id)}
      <div class="approval" data-testid="file-approval">
        <p>
          Relay request <strong>{approval.action}</strong>, since {clock(approval.requested_at_ms, store.nowMs)}{#if approval.description}: {approval.description}{/if}
        </p>
        <p class="muted">It is approved on its card in Board mode.</p>
      </div>
    {/each}
    {#if approvals.length > 0}
      <button type="button" class="action" data-testid="open-in-board" onclick={onBoard}>Open in Board</button>
    {/if}

    {#if card.last_prompt}
      <p class="excerpt prompt" data-testid="file-prompt">{card.last_prompt}</p>
    {/if}
    {#if card.last_message}
      <p class="excerpt" data-testid="file-answer">{card.last_message}</p>
    {/if}
    {#if card.last_message_tail}
      <p class="excerpt tail" data-testid="file-answer-tail">{card.last_message_tail}</p>
    {/if}

    <div class="actions">
      <button
        type="button"
        class="action primary"
        data-testid="bring-to-front"
        onclick={() => store.focus(card!)}
        title={card.kind === "background" ? "Open claude attach" : "Bring the session window to front"}
      >
        Bring to front
      </button>
      {#if card.attach_available}
        <button type="button" class="action" data-testid="copy-attach" onclick={() => store.copyAttach(card!)}>
          <StableLabel labels={{ copied: "Copied", ready: "Copy attach command" }} current={store.copied === card.key ? "copied" : "ready"} />
        </button>
      {/if}
    </div>
    {#if shown}
      <div class="outcome" role="status" data-testid="focus-outcome">
        {outcomeText(shown.outcome)}
        {#if shown.outcome.outcome === "candidates"}
          <ul>
            {#each shown.outcome.detail as title, i (i)}<li>{title}</li>{/each}
          </ul>
        {/if}
        {#if shown.outcome.outcome === "focused"}
          <button type="button" class="link" onclick={() => store.wrongWindow(card!)}>Wrong window?</button>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .file {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 1;
    width: var(--file-width);
    max-width: 100%;
    overflow: auto;
    padding: 16px 20px 20px;
    background: var(--color-surface);
    border-left: 1px solid var(--color-border);
    box-shadow: -6px 0 18px var(--color-shadow);
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }
  h2 {
    flex: 1;
    min-width: 0;
    margin: 8px 0 0;
    font-size: 18px;
    overflow-wrap: anywhere;
  }
  .close,
  .action,
  .row {
    min-height: var(--target-min);
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .close {
    flex: none;
    padding: 0 14px;
  }
  .action {
    padding: 0 18px;
  }
  .primary {
    background: var(--color-text);
    color: var(--color-bg);
    border-color: var(--color-text);
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 14px;
    margin: 14px 0;
  }
  dt {
    color: var(--color-text-muted);
    font-size: 13px;
    padding-top: 1px;
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .dot {
    display: inline-block;
    width: 10px;
    height: 10px;
    margin-right: 6px;
    border-radius: var(--radius-round);
  }
  .priority {
    margin-left: 8px;
    padding: 1px 9px;
    border-radius: var(--radius-control);
    background: var(--color-alert);
    color: var(--color-on-alert);
    font-size: 12px;
    font-weight: 700;
  }
  .pill {
    display: inline-block;
    margin-left: 6px;
    padding: 1px 10px;
    border-radius: var(--radius-control);
    background: var(--color-attention);
    color: var(--color-attention-text);
    font-size: 13px;
    font-weight: 600;
  }
  .approval {
    margin: 0 0 10px;
    padding: 8px 12px;
    border-radius: var(--radius-surface);
    border: 1px solid var(--color-attention-border);
  }
  .approval p {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .excerpt {
    margin: 10px 0 0;
    font-size: 13.5px;
    color: var(--color-text-muted);
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .prompt::before {
    content: "> ";
    font-weight: 700;
  }
  .tail::before {
    content: "... ";
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin-top: 16px;
  }
  .outcome {
    margin-top: 10px;
    padding: 8px 12px;
    border-radius: var(--radius-surface);
    background: var(--color-bg);
    font-size: 13px;
  }
  .outcome ul {
    margin: 4px 0 0;
    padding-left: 18px;
  }
  .link {
    border: 0;
    background: none;
    padding: 0;
    min-height: var(--target-min);
    color: var(--color-working);
    font-weight: 600;
    cursor: pointer;
    text-decoration: underline;
  }
  .muted {
    margin: 0;
    color: var(--color-text-muted);
    font-size: 13px;
  }
  .rows {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 14px;
    border-radius: var(--radius-surface);
    text-align: left;
  }
  .row-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .row-name {
    font-weight: 700;
    overflow-wrap: anywhere;
  }
  .number {
    flex: none;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-round);
    background: var(--color-attention);
    color: var(--color-attention-text);
    font-size: 12.5px;
    font-weight: 800;
  }
</style>
