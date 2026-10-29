<script lang="ts">
  // The orchestrator's proposals (AD-93, FR-12, FR-13): each one under a
  // "proposal" label, with its kind, its status and, when invalid, why.
  // Every text is the orchestrator's and rendered as text (D-130). A
  // proposal acts on nothing: a valid work order proposal opens as a card,
  // which the author launches or not; a pending answer proposal opens its
  // session's meeting, where only the author's click sends it (AD-182); a
  // pending or invalid one can be dismissed. Neutral throughout: no colour,
  // never the attention colour (D-564, D-565). Decided ones are listed
  // apart, without controls.
  import type { DeletedFiles } from "../lib/generated/DeletedFiles";
  import type { ProposalView } from "../lib/generated/ProposalView";
  import {
    PROPOSAL_STATUS_TEXT,
    invalidReasonText,
    proposalKindText,
  } from "../lib/orchestrator-text";
  import { clock } from "../lib/time";

  interface Props {
    proposals: ProposalView[];
    deleted: DeletedFiles;
    /** The open subjects' names by id, for a proposal naming one. */
    subjectNames: Map<number, string>;
    /** The board's session names by session id, for an answer's session. */
    sessionNames: Map<string, string>;
    nowMs: number;
    busy: boolean;
    /** Why the last dismissal failed, and which proposal. */
    error: { id: string; message: string } | null;
    onOpen: (id: string) => void;
    onDismiss: (id: string) => void;
    /** Opens the meeting of the session an answer proposal names. */
    onOpenMeeting: (sessionId: string) => void;
  }
  let { proposals, deleted, subjectNames, sessionNames, nowMs, busy, error, onOpen, onDismiss, onOpenMeeting }: Props =
    $props();

  let open = $derived(proposals.filter((p) => p.status === "pending" || p.status === "invalid"));
  let decided = $derived(proposals.filter((p) => p.status !== "pending" && p.status !== "invalid"));

  function subjectText(id: number): string {
    return subjectNames.get(id) ?? `subject ${id}`;
  }
</script>

{#snippet proposal(item: ProposalView, controls: boolean)}
  <li class="proposal" data-testid="proposal" data-id={item.id} data-status={item.status} data-kind={item.kind ?? "none"}>
    <div class="head">
      <span class="chip" data-testid="proposal-label">proposal</span>
      <strong>{proposalKindText(item.kind)}</strong>
      <span class="muted" data-testid="proposal-status">{PROPOSAL_STATUS_TEXT[item.status]}</span>
      <span class="muted time">{clock(item.received_at_ms ?? item.written_at_ms, nowMs)}</span>
    </div>
    {#if item.invalid_reason !== null}
      <p data-testid="proposal-reason">Invalid: {invalidReasonText(item.invalid_reason)}.</p>
    {/if}
    {#if item.content && "recap" in item.content}
      {@const recap = item.content.recap}
      <p data-testid="proposal-target">
        <span class="label">About</span>
        {#if "subject_id" in recap.target}{subjectText(recap.target.subject_id)}{:else}session
          <span class="text">{recap.target.session_id}</span>{/if}
      </p>
      <p><span class="label">Done</span> <span class="text" data-testid="proposal-text">{recap.done}</span></p>
      <p><span class="label">Blocking</span> <span class="text" data-testid="proposal-text">{recap.blocking}</span></p>
      <p>
        <span class="label">Decision expected</span>
        <span class="text" data-testid="proposal-text">{recap.decision_expected}</span>
      </p>
    {:else if item.content && "work_order" in item.content}
      {@const order = item.content.work_order}
      <p>
        <span class="text" data-testid="proposal-template">{order.template}</span> in
        <span class="text">{order.project}</span>
      </p>
      <ul class="chips" aria-label="Proposed values">
        {#each Object.entries(order.values) as [name, value] (name)}
          <li class="chip" data-testid="proposal-value"><span class="muted">{name}</span> {value}</li>
        {/each}
      </ul>
      <p data-testid="proposal-subject">
        <span class="label">Subject</span>
        {#if "id" in order.subject}{subjectText(order.subject.id)}{:else}a new one; it suggests
          <span class="text">{order.subject.new}</span>{/if}
      </p>
      <p><span class="label">Rationale</span> <span class="text" data-testid="proposal-text">{order.rationale}</span></p>
    {:else if item.content && "answer" in item.content}
      {@const answer = item.content.answer}
      <p data-testid="proposal-session">
        <span class="label">To</span>
        {#if sessionNames.has(answer.session_id)}<span class="text">{sessionNames.get(answer.session_id)}</span>{:else}session
          <span class="text">{answer.session_id}</span>{/if}
      </p>
      <p><span class="label">Answer</span> <span class="text" data-testid="proposal-text">{answer.text}</span></p>
      <p><span class="label">Rationale</span> <span class="text" data-testid="proposal-text">{answer.rationale}</span></p>
    {/if}
    {#if controls}
      <div class="controls">
        {#if item.status === "pending" && item.kind === "work_order"}
          <button type="button" data-testid="proposal-open" disabled={busy} onclick={() => onOpen(item.id)}>
            Open as card
          </button>
        {/if}
        {#if item.status === "pending" && item.content && "answer" in item.content}
          {@const session = item.content.answer.session_id}
          <button type="button" data-testid="proposal-open-meeting" onclick={() => onOpenMeeting(session)}>Open meeting</button>
        {/if}
        <button type="button" data-testid="proposal-dismiss" onclick={() => onDismiss(item.id)}>Dismiss</button>
      </div>
      {#if error?.id === item.id}
        <p class="fault" role="alert" data-testid="proposal-error">The proposal was not dismissed: {error.message}</p>
      {/if}
    {/if}
  </li>
{/snippet}

<section class="block" data-testid="proposals" aria-labelledby="proposals-title">
  <h2 id="proposals-title">Proposals <span class="count">{open.length}</span></h2>
  <p class="muted">
    The orchestrator's suggestions. A proposal runs nothing: a work order proposal opens as a card, which you launch or
    not; a proposed answer waits in its session's meeting, which you send or not.
  </p>
  {#if deleted.misnamed + deleted.over_limit > 0}
    <p class="muted" data-testid="proposals-deleted">
      Deleted unread since the window started: {deleted.misnamed} misnamed {deleted.misnamed === 1 ? "file" : "files"},
      {deleted.over_limit} over the limits.
    </p>
  {/if}
  {#if open.length === 0}
    <p class="muted" data-testid="proposals-none">No proposal waits.</p>
  {:else}
    <ul class="list">
      {#each open as item (item.id)}
        {@render proposal(item, true)}
      {/each}
    </ul>
  {/if}
  {#if decided.length > 0}
    <h3>Decided <span class="count">{decided.length}</span></h3>
    <ul class="list" data-testid="proposals-decided">
      {#each decided as item (item.id)}
        {@render proposal(item, false)}
      {/each}
    </ul>
  {/if}
</section>

<style>
  .block {
    margin: 0 20px;
    padding: 12px 18px;
    border-radius: var(--radius-surface);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }
  h2 {
    margin: 0 0 6px;
    font-size: 16px;
  }
  h3 {
    margin: 12px 0 4px;
    font-size: 14px;
  }
  .count {
    margin-left: 4px;
    color: var(--color-text-muted);
    font-weight: 600;
  }
  p {
    margin: 2px 0;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
  }
  .proposal {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-surface);
    min-width: 0;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
    font-size: 13.5px;
  }
  .label {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-muted);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .chip {
    font-size: 12.5px;
    padding: 1px 7px;
    border-radius: var(--radius-surface);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    unicode-bidi: isolate;
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    /* A bidirectional control in the orchestrator's text stays inside it. */
    unicode-bidi: isolate;
  }
  .time {
    font-variant-numeric: tabular-nums;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 4px;
  }
  .controls button {
    min-height: var(--target-min);
    padding: 0 16px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .controls button:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
