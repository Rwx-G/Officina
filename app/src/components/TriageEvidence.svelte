<script lang="ts">
  // The Triage panel's evidence (FR-10 to FR-12, AD-50, AD-52, AD-55,
  // journey 4): per proposal source, how often its finished was flipped;
  // the auto-archive switch, offered only while its gate holds, and the
  // subjects it archived lately; the triage model, scored by a dry run on
  // the author's own past confirmations without proposing anything, its
  // switch offered only while a recent dry run beat the default and the
  // model can be called, its pause and how to lift it, with the calls made
  // and why any was refused.
  import type { RegistryStore } from "../lib/registry.svelte";
  import {
    DRY_RUN_STOP_TEXT,
    GATE_STATE_TEXT,
    PROPOSAL_SOURCE_TEXT,
    TRIAGE_REFUSAL_TEXT,
    usd,
  } from "../lib/registry-text";
  import { subjectTarget } from "../lib/registry.svelte";
  import { clock, days } from "../lib/time";
  import type { CallCounts } from "../lib/generated/CallCounts";

  interface Props {
    store: RegistryStore;
  }
  let { store }: Props = $props();

  let view = $derived(store.view);
  let model = $derived(view?.model ?? null);
  let latest = $derived(model?.latest_dry_run ?? null);
  let running = $derived(model?.dry_run ?? null);
  let nowMs = $derived(view?.generated_at_ms ?? 0);
  let evaluating = $derived(store.inFlight["evaluate"] ?? false);
  let rates = $derived(view?.rates ?? []);
  let autoArchive = $derived(view?.auto_archive ?? null);
  let gate = $derived(view?.gates.auto_archive ?? null);
  let recent = $derived(view?.recent_auto_archives ?? []);
  /** The switch can always be turned off; on, only while its gate holds. */
  let toggleable = $derived(
    (autoArchive?.on ?? false) || gate?.state === "holds",
  );
  let switching = $derived(store.inFlight["auto-archive"] ?? false);
  let modelGate = $derived(view?.gates.model_enable ?? null);
  /** The model can always be turned off; on, only while it can be called
   *  and its enable gate holds. */
  let modelToggleable = $derived(
    (model?.enabled ?? false) || (model?.unavailable === null && modelGate?.state === "holds"),
  );
  let modelSwitching = $derived(store.inFlight["triage-model"] ?? false);

  /** Like the auto-archive box: it shows what the registry holds, never the
   *  click. */
  function toggleModel(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    const wanted = input.checked;
    input.checked = model?.enabled ?? false;
    void store.setTriageModel(wanted);
  }

  /** The box shows what the registry holds, never the click: it goes back
   *  at once, and the next view moves it when the change was accepted. */
  function toggle(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    const wanted = input.checked;
    input.checked = autoArchive?.on ?? false;
    void store.setAutoArchive(wanted);
  }

  function counted(counts: CallCounts): string {
    const unanswered = counts.unanswered > 0 ? ` (${counts.unanswered} without an answer)` : "";
    return `${counts.calls} ${counts.calls === 1 ? "call" : "calls"}${unanswered}, ${usd(counts.cost_microusd)}`;
  }
</script>

{#if model}
  <section class="evidence" data-testid="triage-evidence" aria-labelledby="evidence-title">
    <h3 id="rates-title">How often each source was wrong</h3>
    <ul class="rates" aria-labelledby="rates-title">
      {#each rates as rate (rate.source)}
        <li data-testid="source-rate" data-source={rate.source}>
          <span class="source">{PROPOSAL_SOURCE_TEXT[rate.source]}</span>:
          {#if rate.finished}
            <span data-testid="rate-finished"
              >{rate.finished.flipped} of its last {rate.finished.cases} finished proposals flipped</span
            >;
          {:else}
            <span data-testid="rate-finished">no finished proposal yet</span>;
          {/if}
          <span data-testid="rate-all">{rate.all.flipped} flipped among its last {rate.all.cases} proposals</span>{#if rate.unsure > 0}<span data-testid="rate-unsure">, {rate.unsure} unsure</span>{/if}.
        </li>
      {/each}
    </ul>
    {#if autoArchive && gate}
      <div class="switch">
        <label>
          <input
            type="checkbox"
            data-testid="auto-archive"
            checked={autoArchive.on}
            disabled={!toggleable || switching}
            onchange={toggle}
          />
          Archive by itself what Claude Code reports done in a filed subject
        </label>
        <p class="muted" data-testid="auto-archive-reason">
          {#if gate.state === "holds"}
            Offered: none of its last {gate.cases} finished proposals was flipped.
          {:else}
            Not offered: {GATE_STATE_TEXT[gate.state]} ({gate.cases} of the {gate.needed} cases it needs).
          {/if}
          {#if !autoArchive.on && autoArchive.src === "rules" && autoArchive.reason}
            It turned itself off: {GATE_STATE_TEXT[autoArchive.reason]}.
          {/if}
        </p>
        {#if store.errors["auto-archive"]}
          <p class="fault" role="alert" data-testid="auto-archive-error">{store.errors["auto-archive"]}</p>
        {/if}
      </div>
    {/if}
    {#if recent.length > 0}
      <ul class="recent" data-testid="recent-auto-archives" aria-label={`Archived by itself in the last ${days(view?.thresholds.auto_count_after_ms ?? 0)}`}>
        {#each recent as archived (archived.subject.id)}
          <li data-testid="auto-archived" data-subject={archived.subject.id}>
            <span class="name">{archived.subject.name}</span>
            <span class="muted">({archived.project}), archived {clock(archived.archived_at_ms, nowMs)}</span>
            {#if archived.archived}
              <button
                class="tool"
                data-testid="auto-archive-reopen"
                disabled={store.inFlight[subjectTarget(archived.subject.id)] ?? false}
                onclick={() => store.reopen(archived.subject.id)}>Reopen</button
              >
            {:else if archived.reopened_at_ms !== null}
              <span class="muted" data-testid="auto-archive-reopened">reopened {clock(archived.reopened_at_ms, nowMs)}</span>
            {/if}
            {#if store.errors[subjectTarget(archived.subject.id)]}
              <span class="fault" role="alert">{store.errors[subjectTarget(archived.subject.id)]}</span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <div class="head">
      <h3 id="evidence-title">Triage model</h3>
      {#if running}
        <p class="progress" role="status" data-testid="dry-run-progress">
          Dry run: {running.done} of {running.total} cases
        </p>
      {:else}
        <button class="tool" data-testid="evaluate" disabled={evaluating} onclick={() => store.evaluate()}>
          Evaluate the model
        </button>
      {/if}
    </div>
    <p class="muted" data-testid="model-state">
      {#if model.enabled}
        The model proposes a verdict where no rule settled one, one case per call; its reason shows as
        text under a "model" label and decides nothing. {model.pending}
        {model.pending === 1 ? "row waits" : "rows wait"} for it.
      {:else}
        The model is off: it proposes nothing. A dry run scores it on your own past confirmations, without
        proposing anything, and costs one call per case.
      {/if}
    </p>
    <div class="switch">
      <label>
        <input
          type="checkbox"
          data-testid="model-toggle"
          checked={model.enabled}
          disabled={!modelToggleable || modelSwitching}
          onchange={toggleModel}
        />
        Let the model propose where no rule settled a row
      </label>
      <p class="muted" data-testid="model-toggle-reason">
        {#if model.unavailable}
          Not offered: {TRIAGE_REFUSAL_TEXT[model.unavailable]}.
        {:else if modelGate?.state === "holds"}
          Offered: the latest dry run, on {modelGate.cases} cases, beat the default.
        {:else if modelGate}
          Not offered: {GATE_STATE_TEXT[modelGate.state]} ({modelGate.cases} of the {modelGate.needed} cases it
          needs).
        {/if}
        {#if !model.enabled && model.src === "rules" && model.reason}
          It turned itself off: {GATE_STATE_TEXT[model.reason]}.
        {/if}
      </p>
      {#if model.enabled && model.paused_at_ms !== null}
        <div class="paused" role="status" data-testid="model-paused">
          <p>
            Paused since {clock(model.paused_at_ms, nowMs)}: its last {view?.thresholds.pause_after_failed_calls}
            calls failed in a row, so nothing is sent. Turn it on again to lift the pause; a new claude lifts it
            too.
          </p>
          <button
            class="tool"
            data-testid="model-reenable"
            disabled={modelSwitching}
            onclick={() => store.setTriageModel(true)}>Turn it on again</button
          >
        </div>
      {/if}
      {#if store.errors["triage-model"]}
        <p class="fault" role="alert" data-testid="model-toggle-error">{store.errors["triage-model"]}</p>
      {/if}
    </div>
    {#if store.errors["evaluate"]}
      <p class="fault" role="alert" data-testid="evaluate-error">{store.errors["evaluate"]}</p>
    {/if}
    {#if model.last_stop}
      <p class="muted" data-testid="dry-run-stop">{DRY_RUN_STOP_TEXT[model.last_stop]}</p>
    {/if}
    {#if latest}
      <div class="run" data-testid="dry-run">
        <p class="muted">Latest dry run, {clock(latest.at_ms, nowMs)}</p>
        <dl>
          <dt>Cases</dt>
          <dd data-testid="dry-run-cases">{latest.tally.cases}</dd>
          <dt>Model right</dt>
          <dd data-testid="dry-run-model-correct">{latest.tally.model_correct}</dd>
          <dt>Default right</dt>
          <dd data-testid="dry-run-default-correct">{latest.tally.default_correct}</dd>
          <dt>Model said finished, you said to resume</dt>
          <dd data-testid="dry-run-false-finished">{latest.tally.model_false_finished}</dd>
          <dt>No answer</dt>
          <dd data-testid="dry-run-unanswered">{latest.tally.unanswered}</dd>
          <dt>Cost</dt>
          <dd data-testid="dry-run-cost">{usd(latest.tally.cost_microusd)}</dd>
        </dl>
      </div>
    {:else}
      <p class="muted" data-testid="no-dry-run">No dry run yet.</p>
    {/if}
    <p class="muted" data-testid="call-counts">
      Today {counted(model.calls.today)}; last {days(view?.thresholds.count_window_ms ?? 0)}
      {counted(model.calls.last_30_days)}.
    </p>
    {#if model.calls.refusals.length > 0}
      <ul class="refusals" data-testid="refusals">
        {#each model.calls.refusals as refusal (refusal.cause)}
          <li data-testid="refusal" data-cause={refusal.cause}>
            Refused {refusal.count} {refusal.count === 1 ? "time" : "times"}, last {clock(refusal.last_at_ms, nowMs)}:
            {TRIAGE_REFUSAL_TEXT[refusal.cause]}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

<style>
  .evidence {
    margin-top: 14px;
    padding-top: 10px;
    border-top: 1px solid var(--color-border);
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  h3 {
    margin: 0;
    font-size: 14.5px;
  }
  .muted {
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .progress {
    margin: 0;
    font-size: 13.5px;
    font-weight: 600;
  }
  .fault {
    font-size: 13px;
  }
  dl {
    display: grid;
    grid-template-columns: max-content max-content;
    gap: 2px 14px;
    margin: 4px 0 0;
    font-size: 13.5px;
  }
  dt {
    color: var(--color-text-muted);
  }
  dd {
    margin: 0;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .refusals,
  .rates,
  .recent {
    margin: 4px 0 0;
    padding-left: 18px;
    font-size: 13px;
  }
  .rates {
    margin-bottom: 10px;
  }
  .recent li {
    margin: 2px 0;
  }
  .source,
  .name {
    font-weight: 600;
  }
  .paused {
    margin-top: 6px;
    padding: 8px 12px;
    border-radius: 12px;
    border: 1px solid var(--color-border);
  }
  .paused p {
    margin: 0 0 6px;
  }
  .switch {
    margin: 8px 0;
    font-size: 13.5px;
  }
  .switch label {
    display: flex;
    gap: 8px;
    align-items: center;
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
</style>
