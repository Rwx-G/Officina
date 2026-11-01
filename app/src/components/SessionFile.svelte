<script lang="ts" module>
  import type { Card } from "../lib/generated/Card";

  /** What the file panel shows: one session's file, a group's list of its
   *  sessions in board order (D-103), or the absent orchestrator's file,
   *  which offers Start (D-567). */
  export type FileForm =
    | { kind: "file"; card: Card; back: boolean }
    | { kind: "list"; title: string; cards: Card[]; focusKey: string | null }
    | { kind: "orchestrator"; title: string; pending: number };
</script>

<script lang="ts">
  // The session file (FR-5, AD-69, D-111): a non-modal side panel at the
  // sessions area's right edge, outside the scene element, so opening it
  // resizes nothing and writes nothing in the scene. It reads the live card
  // and acts on it as its board card does, with the same outcome words
  // (`lib/outcome.ts`). Session text renders as text only.
  import { onMount } from "svelte";
  import { slideMotion } from "../lib/motion";
  import type { Board } from "../lib/generated/Board";
  import type { RelayView } from "../lib/generated/RelayView";
  import { denialsText } from "../lib/relay-text";
  import type { BoardStore } from "../lib/board.svelte";
  import { causeText, waitText } from "../lib/alert";
  import { projectColor } from "../lib/colors";
  import { outcomeText, visibleOutcome } from "../lib/outcome";
  import { liveCards, zoneOf, MAX_NUMBER } from "../lib/scene/layout";
  import { COLUMN_WORDS, BADGE_WORDS, SIGN_WORDS, ZONE_WORDS, agentTypeWords } from "../lib/scene/text";
  import { clock, duration } from "../lib/time";
  import Markdown from "./Markdown.svelte";
  import StableLabel from "./StableLabel.svelte";
  import { meeting } from "../lib/meeting.svelte";
  import type { OrchestratorStore } from "../lib/orchestrator.svelte";
  import { ORCHESTRATOR_ABOUT, pendingText, startAnswerText } from "../lib/orchestrator-text";

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
    /** What the orchestrator's file starts it through (AD-89). */
    orchestrator?: OrchestratorStore;
    /** Shows the Orchestrator view. */
    onOrchestrator?: () => void;
    /** The relay view, for the session's auto-mode denials (story 5.23). */
    relay?: RelayView | null;
  }
  let { form, board, store, onClose, onOpen, onBoard, orchestrator, onOrchestrator, relay = null }: Props = $props();

  const START_LABELS = { start: "Start", starting: "Starting" };
  let startAnswer = $derived(orchestrator?.startError ? `Start failed: ${orchestrator.startError}` : orchestrator?.started ? startAnswerText(orchestrator.started) : null);

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
  // Neutral text, never the attention colour: auto mode decided, and
  // nothing waits on the author (the research note's tier 2).
  let denials = $derived(
    card?.session_id ? (relay?.auto_mode_denials.find((d) => d.session_id === card.session_id) ?? null) : null,
  );

  let panel: HTMLElement;

  // Bring to front has focus when the file opens; a list focuses the row
  // it was left from, else its first row.
  // The orchestrator's file focuses its Close: a key held on the chair
  // never reaches Start (rework F1); Tab does.
  onMount(() => {
    if (form.kind === "file") {
      panel.querySelector<HTMLElement>("[data-testid=bring-to-front]")?.focus();
      return;
    }
    if (form.kind === "orchestrator") {
      panel.querySelector<HTMLElement>("[data-testid=file-close]")?.focus();
      return;
    }
    const rows = [...panel.querySelectorAll<HTMLElement>("[data-row]")];
    const focusKey = form.focusKey;
    (rows.find((row) => row.dataset.row === focusKey) ?? rows[0])?.focus();
  });
</script>

<div class="file" role="dialog" aria-labelledby={titleId} data-testid="session-file" data-form={form.kind} bind:this={panel} in:slideMotion|global={{ x: 16 }}>
  <header>
    <h2 id={titleId}>{form.kind === "file" ? form.card.name : form.title}</h2>
    <button type="button" class="close" data-testid="file-close" onclick={onClose}>
      {form.kind === "file" && form.back ? "Back to the list" : "Close"}
    </button>
  </header>

  {#if form.kind === "orchestrator"}
    <p data-testid="chair-about">{ORCHESTRATOR_ABOUT} It decides nothing that runs: Needs you still waits for you.</p>
    <p class="muted" data-testid="chair-pending">{pendingText(form.pending)}</p>
    <div class="actions">
      <button
        type="button"
        class="action primary"
        data-testid="chair-start"
        disabled={orchestrator?.starting ?? true}
        onclick={() => void orchestrator?.startOrchestrator()}
      >
        <StableLabel labels={START_LABELS} current={orchestrator?.starting ? "starting" : "start"} />
      </button>
      {#if onOrchestrator}
        <button type="button" class="link" data-testid="chair-view" onclick={onOrchestrator}>Open the Orchestrator view</button>
      {/if}
    </div>
    {#if startAnswer}
      <div class="outcome" role="status" data-testid="chair-answer">{startAnswer}</div>
    {/if}
  {:else if form.kind === "list"}
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
      {#if denials && relay && denials.count > 0}
        <dt>Auto mode</dt>
        <dd data-testid="file-denials">{denialsText(denials.count, relay.auto_mode_denials_window_ms)}</dd>
      {/if}
      {#if card.subagents.length > 0}
        <dt>Sub-agents</dt>
        <dd>
          <ul class="subagents" data-testid="file-subagents">
            {#each card.subagents as agent (agent.agent_id)}
              <li>{agentTypeWords(agent)}</li>
            {/each}
          </ul>
        </dd>
      {/if}
    </dl>

    {#each approvals as approval (approval.id)}
      <div class="approval" data-testid="file-approval">
        <p>
          Relay request <strong>{approval.action}</strong> <span data-testid="file-approval-class">(class {approval.class})</span>, since {clock(approval.requested_at_ms, store.nowMs)}{#if approval.description}: {approval.description}{/if}
        </p>
        <p class="muted">It is approved on its card in Board mode.</p>
      </div>
    {/each}
    {#if approvals.length > 0}
      <button type="button" class="action" data-testid="open-in-board" onclick={onBoard}>Open in Board</button>
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
      <button type="button" class="action" data-testid="file-receive" onclick={() => void meeting.receive(card!.key)}>Receive</button>
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

    <!-- The exchange as a conversation (D-705): the author's last prompt on
         the right, Claude's last answer on the left, each labelled by its
         author; a side the card does not carry shows nothing. Last, after
         the actions, as the longest part (D-707). Each message is a safe
         markdown subset (D-1045): text nodes only, no link, no image. -->
    {#if card.last_prompt || card.last_message || card.last_message_tail}
      <ol class="exchange" aria-label="Last exchange" data-testid="file-exchange">
        {#if card.last_prompt}
          <li class="message human" data-testid="file-message" data-author="human">
            <span class="author">Your last prompt</span>
            <div class="text" data-testid="file-prompt"><Markdown text={card.last_prompt} /></div>
          </li>
        {/if}
        {#if card.last_message || card.last_message_tail}
          <li class="message agent" data-testid="file-message" data-author="agent">
            <span class="author">Claude's last answer</span>
            {#if card.last_message}<div class="text" data-testid="file-answer"><Markdown text={card.last_message} /></div>{/if}
            {#if card.last_message_tail}<div class="text" data-testid="file-answer-tail"><Markdown text={card.last_message_tail} /></div>{/if}
          </li>
        {/if}
      </ol>
    {/if}
  {/if}
</div>

<style>
  /* A surface standing in the sessions area (D-702): a gap under the
     header, its right edge on the header's last control, four bordered
     rounded edges, and a shadow the insets hold whole, so nothing of it is
     cut at the area's edge. */
  .file {
    position: absolute;
    top: var(--file-gap);
    right: var(--file-inset);
    bottom: var(--file-inset);
    z-index: 1;
    width: var(--file-width);
    max-width: calc(100% - 2 * var(--file-inset));
    overflow: auto;
    padding: 16px 20px 20px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-surface);
    box-shadow: 0 6px 16px -4px var(--color-shadow);
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
  .action:disabled {
    cursor: default;
    opacity: 0.6;
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
  .subagents {
    margin: 0;
    padding: 0;
    list-style: none;
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
  .exchange {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 16px 0 0;
    padding: 0;
    list-style: none;
  }
  /* A bubble on its author's side, its top corner on that side square, as
     a messaging app draws it. */
  .message {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-width: 85%;
    padding: 8px 12px 10px;
    border-radius: var(--radius-surface);
  }
  .human {
    align-self: flex-end;
    background: var(--color-bubble-human);
    border-top-right-radius: 0;
  }
  .agent {
    align-self: flex-start;
    background: var(--color-bubble-agent);
    border-top-left-radius: 0;
  }
  .author {
    color: var(--color-text-muted);
    font-size: 12px;
    font-weight: 600;
  }
  .human .author {
    text-align: right;
  }
  /* Untrusted text as a safe markdown subset (D-1045), never as HTML. */
  .text {
    min-width: 0;
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
