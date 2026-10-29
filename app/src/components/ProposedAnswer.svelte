<script lang="ts">
  // The orchestrator's proposed answer (AD-182, PRD section 9 item 3,
  // D-488, D-575): beside the author's composer, never copied into it, its
  // text and rationale the orchestrator's, rendered as text. "Send the
  // proposed answer" acts on a pointer click only (D-28): Enter and Space
  // do nothing, and no `tabindex` makes up for it. It carries the
  // proposal's id, never the text, so what runs is the text the collector
  // stored; the same click copies the text, as the author's Send does. The
  // block sits in a reserved slot of fixed size, so a proposal coming,
  // going or being sent moves nothing in the panel. Neutral throughout: a
  // proposal waits on nobody (D-564, D-565).
  import type { MeetingView } from "../lib/generated/MeetingView";
  import { meeting } from "../lib/meeting.svelte";
  import { answerResultText, ANSWER_REFUSAL_TEXT, SEND_PROPOSAL_LABELS } from "../lib/meeting-text";
  import { isPointerClick } from "../lib/pointer";
  import { clock } from "../lib/time";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    view: MeetingView;
    nowMs: number;
  }
  let { view, nowMs }: Props = $props();

  let proposal = $derived(view.proposed_answer);
  let answer = $derived(proposal?.content && "answer" in proposal.content ? proposal.content.answer : null);
  let sending = $derived(proposal !== null && meeting.proposalSending === proposal.id);
  let last = $derived(meeting.proposalResults[view.key] ?? null);
  let error = $derived(proposal && meeting.proposalError?.id === proposal.id ? meeting.proposalError.message : null);

  async function send(event: MouseEvent): Promise<void> {
    if (!isPointerClick(event) || !proposal || !answer || meeting.proposalSending !== null) return;
    // The same gesture copies the text the panel shows, as Send does.
    await navigator.clipboard?.writeText(answer.text).catch(() => undefined);
    await meeting.sendProposal(view.key, view.delivery, proposal.id);
  }

  /** Enter and Space never send (D-28): the click they would synthesise
   *  is stopped before it is made. */
  function noKeySends(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") event.preventDefault();
  }
</script>

<div class="slot" data-stable-slot data-testid="meeting-proposal-slot">
  {#if proposal && answer}
    {@const id = proposal.id}
    <div class="proposal" data-testid="meeting-proposal" data-id={proposal.id}>
      <p class="byline">
        <span class="chip" data-testid="meeting-proposal-label">proposal</span>
        <span class="muted" data-testid="meeting-proposal-origin">an answer from the orchestrator's proposals folder, {clock(proposal.received_at_ms ?? proposal.written_at_ms, nowMs)}</span>
      </p>
      <p class="text answer" data-testid="meeting-proposal-text">{answer.text}</p>
      <p class="why"><span class="label">Why</span> <span class="text" data-testid="meeting-proposal-rationale">{answer.rationale}</span></p>
      <div class="controls">
        <button
          type="button"
          class="send"
          data-testid="proposal-send"
          disabled={sending}
          onclick={(event) => void send(event)}
          onkeydown={noKeySends}
        >
          <StableLabel labels={SEND_PROPOSAL_LABELS} current={sending ? "sending" : "send"} />
        </button>
        <button type="button" class="dismiss" data-testid="meeting-proposal-dismiss" onclick={() => void meeting.dismissProposal(id)}>
          Dismiss
        </button>
      </div>
      {#if error}
        <p class="fault" role="alert" data-testid="meeting-proposal-error">The proposal was not dismissed: {error}</p>
      {/if}
    </div>
  {:else}
    <p class="muted" data-testid="meeting-proposal-none">No answer proposed by the orchestrator.</p>
  {/if}
  {#if last}
    <p class="result" role="status" data-testid="meeting-proposal-result" data-outcome={last.result.outcome}>
      {answerResultText(last.result)}
      {#if last.result.outcome === "refused" && last.result.value.kind === "answer"}{ANSWER_REFUSAL_TEXT[last.result.value.detail]}{/if}
    </p>
  {/if}
</div>

<style>
  /* A fixed box (D-564): what it holds changes, its size never does. */
  .slot {
    box-sizing: border-box;
    height: 15.5rem;
    overflow: auto;
    margin-top: 10px;
    padding: 8px 10px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-surface);
  }
  p {
    margin: 0 0 6px;
  }
  .byline {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
  }
  .chip {
    font-size: 12.5px;
    padding: 1px 7px;
    border-radius: var(--radius-surface);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    /* A bidirectional control in the orchestrator's text stays inside it. */
    unicode-bidi: isolate;
  }
  .answer {
    max-height: 6.5rem;
    overflow: auto;
    padding: 6px 8px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    font-size: 14px;
  }
  .why {
    font-size: 13px;
  }
  .label {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-muted);
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
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
  .controls .dismiss {
    border-color: var(--color-border);
  }
  .controls button:disabled {
    cursor: progress;
  }
  .muted {
    color: var(--color-text-muted);
    font-size: 13px;
  }
  .result {
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
