<script lang="ts">
  // The author's answer (FR-6, AD-85, AD-93, D-168, D-564): a textarea never
  // prefilled, its count against the limit the payload carries, and Send,
  // reachable by keyboard and Ctrl+Enter since an answer approves nothing.
  // Send copies the text on the same gesture whatever the delivery; the
  // draft stays in memory, per card key, until confirmed or discarded.
  import type { MeetingView } from "../lib/generated/MeetingView";
  import { meeting } from "../lib/meeting.svelte";
  import { ANSWER_REFUSAL_TEXT, answerResultText, SEND_LABELS } from "../lib/meeting-text";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    view: MeetingView;
  }
  let { view }: Props = $props();

  const fieldId = $props.id();
  let text = $derived(meeting.draftOf(view.key));
  let count = $derived([...text].length);
  let over = $derived(count > view.answer_max_chars);
  let sending = $derived(meeting.sending === view.key);
  let result = $derived(meeting.resultOf(view.key));

  async function send(): Promise<void> {
    if (sending || text.trim() === "") return;
    // The same gesture copies: a delivery that copies has its text ready,
    // and a resume keeps it in case it is not confirmed.
    await navigator.clipboard?.writeText(text).catch(() => undefined);
    await meeting.send(view.key, view.delivery, text);
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key === "Enter" && event.ctrlKey && !event.altKey && !event.metaKey) {
      event.preventDefault();
      void send();
    }
  }
</script>

<div class="composer" data-testid="answer-composer">
  <label for={fieldId}>Your answer</label>
  <textarea
    id={fieldId}
    data-testid="answer-text"
    rows="4"
    value={text}
    oninput={(event) => meeting.setDraft(view.key, (event.currentTarget as HTMLTextAreaElement).value)}
    onkeydown={onKey}
  ></textarea>
  <div class="row">
    <span class="count" class:over data-testid="answer-count">{count} / {view.answer_max_chars}</span>
    <button type="button" class="send" data-testid="answer-send" disabled={sending} onclick={() => void send()}>
      <StableLabel labels={SEND_LABELS} current={sending ? "sending" : "send"} />
    </button>
  </div>
  {#if result}
    <p class="result" role="status" data-testid="answer-result" data-outcome={result.outcome}>
      {answerResultText(result)}
      {#if result.outcome === "refused" && result.value.kind === "answer"}{ANSWER_REFUSAL_TEXT[result.value.detail]}{/if}
    </p>
    {#if result.outcome === "not_confirmed" || result.outcome === "refused" || result.outcome === "spawn_failed"}
      <button type="button" class="link" data-testid="answer-discard" onclick={() => meeting.discard(view.key)}>Discard the draft</button>
    {/if}
  {/if}
</div>

<style>
  .composer {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  label {
    font-size: 13px;
    color: var(--color-text-muted);
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    padding: 8px 10px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
    font-size: 14px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .count {
    font-size: 12.5px;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .count.over {
    color: var(--color-danger);
    font-weight: 700;
  }
  .send {
    min-height: var(--target-min);
    padding: 0 18px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-text);
    background: var(--color-text);
    color: var(--color-bg);
    font-weight: 600;
    cursor: pointer;
  }
  .send:disabled {
    cursor: progress;
  }
  .result {
    margin: 0;
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .link {
    align-self: flex-start;
    border: 0;
    background: none;
    padding: 0;
    min-height: 32px;
    color: var(--color-working);
    font-weight: 600;
    cursor: pointer;
    text-decoration: underline;
  }
</style>
