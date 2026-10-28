<script lang="ts">
  // One row that needs a verdict (FR-9, AD-55): the proposal with its
  // source and reason, the excerpts it rests on, the verdict the author
  // sets, and for an unfiled row the subject it goes under. An empty
  // subject is offered for archiving. A row queued for the triage model says
  // so; the model's reason, untrusted, shows as text under a "model" label;
  // a call that gave no answer says why the default stands. A refusal shows
  // on the row, which stays. Session, subject and model text renders as
  // text.
  import type { TriageRow } from "../lib/generated/TriageRow";
  import type { Verdict } from "../lib/generated/Verdict";
  import type { RegistryStore } from "../lib/registry.svelte";
  import {
    PROPOSAL_SOURCE_TEXT,
    PROPOSED_VERDICT_TEXT,
    REASON_TEXT,
    TRIAGE_OUTCOME_TEXT,
    TRIAGE_REFUSAL_TEXT,
    VERDICT_TEXT,
  } from "../lib/registry-text";
  import SubjectPicker, { type PickerChoice } from "./SubjectPicker.svelte";

  interface Props {
    row: TriageRow;
    store: RegistryStore;
  }
  let { row, store }: Props = $props();

  const VERDICTS: Verdict[] = ["finished", "to_resume"];

  let verdict = $derived(store.verdictOf(row));
  let target = $derived(store.fileUnderOf(row));
  let inFlight = $derived(store.confirming[row.key] ?? false);
  let error = $derived(store.rowErrors[row.key] ?? null);
  let title = $derived(
    row.subject?.name ?? row.session_name ?? (row.session_id ? `session ${row.session_id.slice(-8)}` : row.key),
  );
  let answerEnd = $derived(row.excerpts?.answer_tail ?? row.excerpts?.answer_head ?? null);
  let choices = $derived(row.choices.map((choice) => ({ ...choice, project: row.project })));
  let targetText = $derived.by(() => {
    if (target === null) return "No subject chosen yet.";
    if ("new" in target) return `Files under a new subject: ${target.new.name}`;
    const id = target.subject.id;
    return `Files under ${row.choices.find((c) => c.id === id)?.name ?? `subject ${id}`}`;
  });

  function pick(choice: PickerChoice): void {
    store.fileUnder(row.key, "subject" in choice ? { subject: { id: choice.subject } } : { new: { name: choice.name } });
  }
</script>

<article class="row" class:in-flight={inFlight} data-testid="triage-row" data-key={row.key} data-kind={row.kind}>
  <header>
    <h3 class="title" data-testid="triage-title">{title}</h3>
    <bdi class="project">{row.project}</bdi>
    {#if row.kind === "unfiled_session"}<span class="tag">unfiled</span>{/if}
    {#if row.model_pending}<span class="tag" data-testid="triage-model-pending">model pending</span>{/if}
    {#if inFlight}<span class="tag" data-testid="triage-in-flight">confirming...</span>{/if}
  </header>

  {#if row.kind === "empty_subject"}
    <p class="empty" data-testid="triage-empty">empty subject: archive?</p>
  {/if}

  <p class="proposal" data-testid="triage-proposal">
    Proposed: <strong data-testid="triage-proposed">{PROPOSED_VERDICT_TEXT[row.proposal.verdict]}</strong>
    <span class="source" data-testid="triage-source">{PROPOSAL_SOURCE_TEXT[row.proposal.source]}</span>
  </p>
  <p class="reason" data-testid="triage-reason">{REASON_TEXT[row.proposal.reason]}</p>
  {#if row.model_reason !== null}
    <p class="model" data-testid="triage-model-reason">
      <span class="label" data-testid="triage-model-label">model</span>
      <span data-testid="triage-model-text">{row.model_reason}</span>
    </p>
  {/if}
  {#if row.model_call}
    <p class="muted" data-testid="triage-model-call">
      The model gave no answer: {TRIAGE_OUTCOME_TEXT[row.model_call.outcome]}{#if row.model_call.refusal}
        ({TRIAGE_REFUSAL_TEXT[row.model_call.refusal]}){/if}. The default stands.
    </p>
  {/if}

  {#if row.excerpts?.prompt || answerEnd}
    <div class="excerpts">
      {#if row.excerpts?.prompt}<p class="prompt" data-testid="triage-prompt">{row.excerpts.prompt}</p>{/if}
      {#if answerEnd}<p class="answer" data-testid="triage-answer">{answerEnd}</p>{/if}
    </div>
  {/if}

  {#if row.kind === "unfiled_session"}
    <div class="filing" data-testid="triage-filing">
      <p data-testid="triage-file-under">{targetText}</p>
      <SubjectPicker
        subjects={choices}
        project={row.project}
        label="File {title} under"
        disabled={inFlight}
        selected={target !== null && "subject" in target ? target.subject.id : null}
        onPick={pick}
      />
    </div>
  {/if}

  <div class="verdict" role="group" aria-label="Verdict">
    {#each VERDICTS as option (option)}
      <button
        class="tool"
        data-testid="triage-verdict-{option}"
        aria-pressed={verdict === option}
        disabled={inFlight}
        onclick={() => store.setVerdict(row.key, option)}
      >
        {VERDICT_TEXT[option]}
      </button>
    {/each}
    {#if verdict === null}
      <span class="muted" data-testid="triage-no-verdict">Press f or t to set the verdict.</span>
    {/if}
  </div>

  {#if verdict === "finished"}
    <label class="routine">
      <input
        type="checkbox"
        data-testid="triage-routine"
        checked={store.routines[row.key] ?? false}
        disabled={inFlight}
        onchange={(event) => store.setRoutine(row.key, (event.currentTarget as HTMLInputElement).checked)}
      />
      Routine, could have run alone
    </label>
  {/if}

  <button
    class="tool confirm"
    data-testid="triage-confirm"
    disabled={inFlight || store.item(row) === null}
    onclick={() => store.confirm([row])}
  >
    Confirm
  </button>

  {#if error}
    <p class="fault" role="alert" data-testid="triage-error">{error}</p>
  {/if}
</article>

<style>
  .row {
    padding: 12px 18px;
    border-radius: 16px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }
  .row.in-flight {
    opacity: 0.6;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 6px 12px;
  }
  h3 {
    margin: 0;
    font-size: 16px;
    overflow-wrap: anywhere;
  }
  .project,
  .muted,
  .source {
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .tag {
    font-size: 12px;
    font-weight: 600;
    padding: 1px 9px;
    border-radius: var(--radius-pill);
    background: var(--color-border);
  }
  p {
    margin: 6px 0 0;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .empty {
    font-weight: 600;
  }
  .model {
    color: var(--color-text-muted);
  }
  .model .label {
    font-size: 12px;
    font-weight: 700;
    padding: 1px 8px;
    margin-right: 6px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-border);
  }
  .excerpts p {
    color: var(--color-text-muted);
  }
  .prompt::before {
    content: "> ";
    font-weight: 700;
  }
  .answer {
    font-style: italic;
  }
  .filing,
  .verdict {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
  }
  .filing {
    flex-direction: column;
    align-items: flex-start;
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
  .tool[aria-pressed="true"] {
    background: var(--color-text);
    color: var(--color-bg);
  }
  .confirm {
    margin-top: 10px;
  }
  .routine {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    font-size: 13.5px;
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
