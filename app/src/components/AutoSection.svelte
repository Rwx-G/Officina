<script lang="ts">
  // The Relay view's `auto` section (AD-88, AD-92, AD-93, FR-9): what the
  // shadow reviews say against the author's decisions and how weak that
  // evidence still is, the approval toggle offered only while the gate
  // holds, the rule offer only while it holds, the runs a review or the
  // rule approved with "should not have run", and the decisions per day.
  // Every number comes from the payload; nothing is computed here (AD-5).
  // Turning approval or rule mode on, and a mark, act on a pointer click
  // only (D-28); turning off is reachable by keyboard. The toggles keep
  // their box in every state and sit in slots the section reserves, so
  // their coming and going moves nothing (D-564).
  import type { AutoView } from "../lib/generated/AutoView";
  import { isPointerClick } from "../lib/pointer";
  import type { RelayStore } from "../lib/relay.svelte";
  import {
    APPROVED_BY_TEXT,
    AUTO_APPROVAL_TOGGLE_TEXT,
    AUTO_MODE_TEXT,
    AUTO_MODE_TOGGLE_TEXT,
    AUTO_OFF_TEXT,
    boundText,
    reviewText,
  } from "../lib/relay-text";
  import { ago, days } from "../lib/time";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    store: RelayStore;
    auto: AutoView;
    nowMs: number;
  }
  let { store, auto, nowMs }: Props = $props();

  let stats = $derived(auto.stats);
  /** The toggle is offered while the gate holds, and to turn approval off. */
  let toggleShown = $derived(auto.approval_on || stats.gate);
  /** The rule offer, and the way back to the review from rule mode. */
  let offerShown = $derived(auto.mode === "rule" || stats.rule_offer);

  function toggleApproval(event: MouseEvent): void {
    const turningOn = !auto.approval_on;
    if (turningOn && !isPointerClick(event)) return;
    void store.setAutoApproval(turningOn);
  }

  function toggleMode(event: MouseEvent): void {
    const toRule = auto.mode === "review";
    if (toRule && !isPointerClick(event)) return;
    void store.setAutoMode(toRule ? "rule" : "review");
  }

  function mark(event: MouseEvent, runId: string): void {
    if (!isPointerClick(event)) return;
    void store.markShouldNotHaveRun(runId);
  }

  /** Enter and Space never turn a switch on (D-28). */
  function noKeyTurnsOn(event: KeyboardEvent, on: boolean): void {
    if (!on && (event.key === "Enter" || event.key === " ")) event.preventDefault();
  }
</script>

<section class="block" data-testid="relay-auto" aria-labelledby="relay-auto-title">
  <h2 id="relay-auto-title">Auto approval</h2>
  <p data-testid="auto-state">
    {#if auto.approval_on}
      On, mode {auto.mode}: {AUTO_MODE_TEXT[auto.mode]}.
    {:else}
      Off: each review runs in shadow and approves nothing{auto.approval_reason
        ? `; turned off because ${AUTO_OFF_TEXT[auto.approval_reason]}`
        : ""}.
    {/if}
  </p>
  <p data-testid="auto-counts">
    {stats.cases} shadow {stats.cases === 1 ? "case" : "cases"} met your decision in the last 30 days, {stats.disagreements}
    {stats.disagreements === 1 ? "disagreement" : "disagreements"}; review and rule differ in {stats.rule_diff}. The gate
    needs {stats.gate_cases} cases and no disagreement.
  </p>
  <p class="muted" data-testid="auto-bound">{boundText(stats)}</p>
  <p class="muted" data-testid="auto-budget">
    Review model {auto.review_model ?? "none (the [auto_review] table is invalid)"}; today {auto.reviews_today}
    {auto.reviews_today === 1 ? "request" : "requests"}, {auto.calls_today} of {auto.calls_per_day} calls.
  </p>

  <div class="slot" data-stable-slot data-testid="auto-approval-slot" class:shown={toggleShown}>
    <div class="slot-inner" inert={!toggleShown} aria-hidden={!toggleShown}>
      <button
        class="tool"
        data-testid="auto-approval-toggle"
        aria-pressed={auto.approval_on}
        disabled={store.autoBusy}
        onclick={toggleApproval}
        onkeydown={(event) => noKeyTurnsOn(event, auto.approval_on)}
      >
        <StableLabel labels={AUTO_APPROVAL_TOGGLE_TEXT} current={auto.approval_on ? "on" : "off"} />
      </button>
    </div>
  </div>

  <div class="slot" data-stable-slot data-testid="auto-mode-slot" class:shown={offerShown}>
    <div class="slot-inner" inert={!offerShown} aria-hidden={!offerShown}>
      <button
        class="tool"
        data-testid="auto-mode-toggle"
        aria-pressed={auto.mode === "rule"}
        disabled={store.autoBusy}
        onclick={toggleMode}
        onkeydown={(event) => noKeyTurnsOn(event, auto.mode === "rule")}
      >
        <StableLabel labels={AUTO_MODE_TOGGLE_TEXT} current={auto.mode} />
      </button>
      <span class="muted">Review and rule differ in under {stats.rule_diff_percent} % of the cases.</span>
    </div>
  </div>
  {#if store.autoError}
    <p class="fault" role="alert" data-testid="auto-error">{store.autoError}</p>
  {/if}

  <h3>Runs approved without you, last {days(auto.mark_window_ms)}</h3>
  {#if auto.runs.length === 0}
    <p class="muted" data-testid="auto-runs-empty">None.</p>
  {:else}
    <ul class="runs">
      {#each auto.runs as run (run.approval_id)}
        <li data-testid="auto-run" data-id={run.approval_id}>
          <span><strong>{run.project}</strong> {run.action}, by {APPROVED_BY_TEXT[run.by]}, {ago(run.requested_at_ms, nowMs)}</span>
          <span class="muted">{reviewText(run.review)}</span>
          {#if run.marked_at_ms !== null}
            <span class="fault" data-testid="auto-run-marked">marked: should not have run</span>
          {:else if run.run_id !== null}
            <button
              class="tool"
              data-testid="mark-run"
              disabled={store.autoBusy}
              onclick={(event) => mark(event, run.run_id!)}
              onkeydown={(event) => noKeyTurnsOn(event, false)}
            >
              Should not have run
            </button>
          {:else}
            <span class="muted">not run yet</span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  <h3>Decisions per day</h3>
  <table class="days" data-testid="auto-decisions">
    <thead>
      <tr>
        <th scope="col">Day</th>
        <th scope="col">Yours</th>
        <th scope="col">Auto</th>
      </tr>
    </thead>
    <tbody>
      {#each auto.decisions as day (day.day)}
        <tr data-testid="auto-day">
          <td>{day.day}</td>
          <td>{day.author}</td>
          <td>{day.auto}</td>
        </tr>
      {/each}
    </tbody>
  </table>
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
    margin: 10px 0 4px;
    font-size: 14px;
  }
  p {
    margin: 4px 0;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
  /* Reserved whatever its state, so a toggle coming or going moves
     nothing (D-564). */
  .slot {
    min-height: calc(var(--target-min) + 6px);
  }
  .slot-inner {
    display: flex;
    align-items: center;
    gap: 10px;
    visibility: hidden;
  }
  .slot.shown .slot-inner {
    visibility: visible;
  }
  .tool {
    margin-top: 6px;
    min-height: var(--target-min);
    padding: 0 18px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .tool[aria-pressed="true"] {
    background: var(--color-bg);
  }
  .tool:disabled {
    cursor: progress;
    opacity: 0.6;
  }
  .runs {
    margin: 0;
    padding-left: 18px;
    font-size: 13.5px;
  }
  .runs li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    margin: 3px 0;
    overflow-wrap: anywhere;
  }
  .runs .tool {
    margin-top: 0;
  }
  .days {
    border-collapse: collapse;
    font-size: 13px;
  }
  .days th,
  .days td {
    padding: 2px 10px 2px 0;
    text-align: left;
    border-bottom: 1px solid var(--color-border);
  }
  .days th {
    color: var(--color-text-muted);
    font-weight: 600;
  }
</style>
