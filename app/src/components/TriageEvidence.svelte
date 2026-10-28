<script lang="ts">
  // The Triage panel's evidence on the triage model (FR-10, AD-52, AD-55,
  // journey 4): the model stays off; a dry run scores it on the author's
  // own past confirmations, never proposing anything; the panel shows the
  // latest one, the one running, the calls made and why any was refused.
  import type { RegistryStore } from "../lib/registry.svelte";
  import { DRY_RUN_STOP_TEXT, TRIAGE_REFUSAL_TEXT, usd } from "../lib/registry-text";
  import { clock } from "../lib/time";
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

  function counted(counts: CallCounts): string {
    const unanswered = counts.unanswered > 0 ? ` (${counts.unanswered} without an answer)` : "";
    return `${counts.calls} ${counts.calls === 1 ? "call" : "calls"}${unanswered}, ${usd(counts.cost_microusd)}`;
  }
</script>

{#if model}
  <section class="evidence" data-testid="triage-evidence" aria-labelledby="evidence-title">
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
        The model proposes a verdict where no rule settled one.
      {:else}
        The model is off: it proposes nothing. A dry run scores it on your own past confirmations, without
        proposing anything, and costs one call per case.
      {/if}
    </p>
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
      Today {counted(model.calls.today)}; last 30 days {counted(model.calls.last_30_days)}.
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
  .refusals {
    margin: 4px 0 0;
    padding-left: 18px;
    font-size: 13px;
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
