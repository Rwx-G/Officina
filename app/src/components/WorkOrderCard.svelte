<script lang="ts">
  // A work order card (AD-78, AD-35, FR-4): what will run, exactly as core
  // built it. The template and its description, the project, each value as
  // a chip, the whole rendered prompt as text, the argument vector one chip
  // per argument, the folder, the subject and the quota windows. Launch
  // acts on a pointer click only (D-28): Enter and Space do nothing, and no
  // `tabindex` makes up for it (FR-13 as amended). The launch's outcome
  // stays on the card; the bound session, the exit code and the error come
  // from the history's row for the card. Nothing on the card is amber: it
  // is the author's own order, not something waiting on him.
  import type { LaunchAnswer } from "../lib/generated/LaunchAnswer";
  import type { WorkOrderCard } from "../lib/generated/WorkOrderCard";
  import type { WorkOrderView } from "../lib/generated/WorkOrderView";
  import { LAUNCH_REFUSAL_TEXT, PHASE_TEXT, launchOutcomeText } from "../lib/orchestrator-text";
  import { isPointerClick } from "../lib/pointer";
  import Gauge from "./Gauge.svelte";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    card: WorkOrderCard;
    /** The history's row for this card, once the launch recorded one. */
    order: WorkOrderView | null;
    launching: boolean;
    launch: LaunchAnswer | null;
    nowMs: number;
    /** Launches the card: called on a pointer click only. */
    onLaunch: () => void;
  }
  let { card, order, launching, launch, nowMs, onLaunch }: Props = $props();

  const LAUNCH_LABELS = { idle: "Launch", launching: "Launching" } as const;

  let subject = $derived.by(() => {
    const chosen = card.subject;
    if (chosen.kind === "new") return `a new subject, ${chosen.value}`;
    return card.subject_options.find((s) => s.id === chosen.value)?.name ?? `subject ${chosen.value}`;
  });
  /** The outcome shown: the click's answer, or the history's once the
   *  answer came back before the outcome did. */
  let outcome = $derived.by(() => {
    if (launch?.outcome !== "launched") return null;
    return launch.value === "launching" && order ? order.outcome : launch.value;
  });
  let done = $derived(launch !== null || card.state !== "open");

  function clicked(event: MouseEvent): void {
    if (!isPointerClick(event) || launching || done) return;
    onLaunch();
  }

  /** Enter and Space never launch (D-28): the click they would synthesise
   *  is stopped before it is made. */
  function noKeyLaunches(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") event.preventDefault();
  }
</script>

<article class="card" data-testid="work-order-card" data-card={card.id} data-state={card.state} aria-label="Work order {card.name}">
  <div class="head">
    <strong data-testid="card-template">{card.template}</strong>
    <span class="chip" data-testid="card-project">{card.project}</span>
    <span class="muted" data-testid="card-name">{card.name}</span>
  </div>
  <p class="muted" data-testid="card-description">{card.description}</p>
  {#if card.values.length > 0}
    <ul class="chips" aria-label="Values">
      {#each card.values as param (param.name)}
        <li class="chip" data-testid="card-value" data-param={param.name}>
          <span class="muted">{param.name}</span>
          {param.value}
        </li>
      {/each}
    </ul>
  {/if}
  <div class="part">
    <span class="label">Prompt</span>
    <pre data-testid="card-prompt">{card.prompt}</pre>
  </div>
  <div class="part">
    <span class="label">Runs</span>
    <ol class="chips" data-testid="card-argv" aria-label="The arguments after claude, one per chip">
      {#each card.argv as arg, i (i)}
        <li class="chip arg" data-testid="card-arg">{arg}</li>
      {/each}
    </ol>
  </div>
  <p data-testid="card-cwd"><span class="label">In</span> <code>{card.cwd}</code></p>
  <p data-testid="card-subject"><span class="label">Filed under</span> {subject}</p>
  <div class="gauges" aria-label="Quota when the card was issued">
    <Gauge
      label="5 h"
      window={card.quota?.five_hour ?? null}
      weekly={false}
      writtenAtMs={card.quota?.written_at_ms ?? null}
      staleAfterMs={card.quota?.stale_after_ms ?? 0}
      {nowMs}
    />
    <Gauge
      label="Week"
      window={card.quota?.seven_day ?? null}
      weekly={true}
      writtenAtMs={card.quota?.written_at_ms ?? null}
      staleAfterMs={card.quota?.stale_after_ms ?? 0}
      {nowMs}
    />
  </div>
  <div class="launch">
    <button
      type="button"
      data-testid="launch"
      disabled={launching || done}
      onclick={clicked}
      onkeydown={noKeyLaunches}
    >
      <StableLabel labels={LAUNCH_LABELS} current={launching ? "launching" : "idle"} />
    </button>
    <span class="muted">The session runs as you, in the project's folder, under its own settings.</span>
  </div>
  {#if launching}
    <p data-testid="card-launching">Launching: waiting for claude.</p>
  {:else if launch?.outcome === "refused"}
    <p class="fault" role="alert" data-testid="card-launch-refused">{LAUNCH_REFUSAL_TEXT[launch.value]}</p>
  {:else if outcome !== null}
    <div data-testid="card-outcome" data-outcome={outcome}>
      <p class:fault={outcome !== "started"}>
        {launchOutcomeText(outcome, card.cwd)}{#if order && order.exit_code !== null && outcome !== "started"}
          Exit code {order.exit_code}.{/if}
      </p>
      {#if outcome === "started"}
        <p data-testid="card-session">
          {#if order?.session}
            Its session: {order.session.name ?? order.session.session_id}, {PHASE_TEXT[order.session.phase]}.
          {:else if order?.unbound}
            No background session took its name within ten minutes.
          {:else}
            <span class="muted">Waiting for its background session.</span>
          {/if}
        </p>
      {/if}
      {#if order && order.error !== null}
        <pre data-testid="card-error">{order.error}</pre>
      {/if}
    </div>
  {/if}
</article>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0 20px;
    padding: 12px 18px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-surface);
    min-width: 0;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
  }
  .head strong {
    font-size: 16px;
    overflow-wrap: anywhere;
  }
  p {
    margin: 0;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .part {
    display: flex;
    flex-direction: column;
    gap: 2px;
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
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
    padding: 1px 7px;
    border-radius: var(--radius-surface);
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    /* A bidirectional control in one argument stays inside its chip. */
    unicode-bidi: isolate;
  }
  pre {
    margin: 0;
    max-height: 220px;
    overflow: auto;
    padding: 6px 8px;
    border-radius: var(--radius-surface);
    background: var(--color-bg);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    unicode-bidi: isolate;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .gauges {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .launch {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
  }
  .launch button {
    min-height: var(--target-min);
    padding: 0 22px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 700;
    cursor: pointer;
  }
  .launch button:disabled {
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
