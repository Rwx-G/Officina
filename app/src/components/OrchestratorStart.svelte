<script lang="ts">
  // The orchestrator's presence and its Start (AD-89, AD-93, D-564): live
  // with its status while an agents entry shows it, absent otherwise, and
  // one button that starts it or, while it is live, brings its console to
  // the front. The button keeps its box through StableLabel and takes no
  // colour of its own; what the last Start answered shows beside it.
  import type { OrchestratorStore } from "../lib/orchestrator.svelte";
  import { LIVE_STATUS_TEXT, ORCHESTRATOR_ABOUT, startAnswerText } from "../lib/orchestrator-text";
  import { clock } from "../lib/time";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    store: OrchestratorStore;
    nowMs: number;
  }
  let { store, nowMs }: Props = $props();

  const START_LABELS = { start: "Start", starting: "Starting", front: "Bring to front" };

  let presence = $derived(store.view?.presence ?? null);
  let state = $derived<keyof typeof START_LABELS>(store.starting ? "starting" : presence ? "front" : "start");
  let presenceText = $derived.by(() => {
    if (!presence) return "Absent.";
    const status = presence.status === null ? "status unknown" : LIVE_STATUS_TEXT[presence.status];
    const since = presence.started_at_ms === null ? "" : `, started ${clock(presence.started_at_ms, nowMs)}`;
    return `Live: ${status}${since}.`;
  });
  let answer = $derived(store.started ? startAnswerText(store.started) : null);
  let answerIsFault = $derived(
    store.startError !== null || store.started?.outcome === "refused" || store.started?.outcome === "failed",
  );
</script>

<section class="block" data-testid="orchestrator-presence-block" aria-labelledby="orchestrator-title">
  <h2 id="orchestrator-title">Orchestrator</h2>
  <p data-testid="orchestrator-presence-text" data-presence={presence ? "live" : "absent"}>{presenceText}</p>
  <div class="start">
    <button
      type="button"
      data-testid="orchestrator-start"
      disabled={store.starting}
      onclick={() => void store.startOrchestrator()}
    >
      <StableLabel labels={START_LABELS} current={state} />
    </button>
    {#if store.startError}
      <span class="fault" role="alert" data-testid="orchestrator-start-answer">Start failed: {store.startError}</span>
    {:else if answer}
      <span class:fault={answerIsFault} role="status" data-testid="orchestrator-start-answer">{answer}</span>
    {:else}
      <span class="muted">{ORCHESTRATOR_ABOUT}</span>
    {/if}
  </div>
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
  p {
    margin: 4px 0;
    font-size: 13.5px;
  }
  .start {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin-top: 6px;
    font-size: 13.5px;
  }
  .start button {
    min-height: var(--target-min);
    padding: 0 22px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 700;
    cursor: pointer;
  }
  .start button:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
    overflow-wrap: anywhere;
  }
</style>
