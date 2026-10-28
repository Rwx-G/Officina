<script lang="ts">
  // The Relay view (AD-35, FR-10): the stop toggle, the pending change with
  // its Approve button, the floor's issues, the approved catalogue, the
  // recent activity and the journal's own diagnostics. Approvals of `human`
  // actions are not here: they are decided on their cards in Needs you.
  // Every text is rendered as text.
  import { isPointerClick } from "../lib/pointer";
  import type { RelayStore } from "../lib/relay.svelte";
  import { ANOMALY_TEXT, RELAY_TOGGLE_TEXT } from "../lib/relay-text";
  import { clock, hours } from "../lib/time";
  import CatalogueTable from "./CatalogueTable.svelte";
  import PendingCatalogue from "./PendingCatalogue.svelte";
  import RecentActivity from "./RecentActivity.svelte";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    store: RelayStore;
    nowMs: number;
  }
  let { store, nowMs }: Props = $props();

  let relay = $derived(store.relay);
  let open = $derived(relay?.anomalies.filter((a) => !a.acknowledged).length ?? 0);
</script>

<main class="relay-view" data-testid="relay-view">
  <section class="block" data-testid="relay-stop" aria-labelledby="relay-stop-title">
    <h2 id="relay-stop-title">Relay</h2>
    <p data-testid="relay-state">
      {#if relay?.stopped}
        Stopped{relay.stopped_since_ms !== null ? ` since ${clock(relay.stopped_since_ms, nowMs)}` : ""}:
        <code>officina run</code> refuses every action until you resume it. The gate still guards your projects.
      {:else}
        Running: <code>officina run</code> serves the approved catalogue.
      {/if}
    </p>
    <button
      class="tool"
      data-testid="relay-view-stop"
      aria-pressed={store.stopped}
      disabled={store.switching}
      onclick={() => store.setStopped(!store.stopped)}
    >
      <StableLabel labels={RELAY_TOGGLE_TEXT} current={store.stopped ? "stopped" : "running"} />
    </button>
    {#if store.stopError}
      <p class="fault" role="alert">The relay could not be switched: {store.stopError}</p>
    {/if}
    {#if relay?.floor}
      <p class="muted">Relay floor: <code data-testid="relay-floor">{relay.floor}</code></p>
    {/if}
    {#if relay?.floor_error}
      <p class="fault" role="alert" data-testid="relay-view-floor-error">
        Relay floor cannot be used: {relay.floor_error}.
      </p>
    {/if}
    {#if relay?.snapshot_error}
      <p class="fault" role="alert">The approved catalogue cannot be read ({relay.snapshot_error}).</p>
    {/if}
  </section>

  {#if relay && relay.anomalies.length > 0}
    <section class="block" data-testid="relay-anomalies" aria-labelledby="relay-anomalies-title">
      <h2 id="relay-anomalies-title">Decisions this window did not make</h2>
      <p class="muted">
        {open} to acknowledge. A file another process wrote can stand for an approval, a catalogue or a resume made here;
        doctor fails until you acknowledge each one you have traced.
      </p>
      <ul class="issues">
        {#each relay.anomalies as anomaly (anomaly.key)}
          <li data-testid="relay-anomaly" data-kind={anomaly.kind} class:fault={!anomaly.acknowledged}>
            <strong>{ANOMALY_TEXT[anomaly.kind]}</strong>{anomaly.at_ms !== null ? `, ${clock(anomaly.at_ms, nowMs)}` : ""}:
            {anomaly.detail}
            {#if anomaly.acknowledged}
              <span class="muted" data-testid="anomaly-acknowledged">(acknowledged)</span>
            {:else}
              <button
                class="tool"
                data-testid="acknowledge-anomaly"
                disabled={store.acknowledging[anomaly.key] === true}
                onclick={(event) => {
                  if (isPointerClick(event)) void store.acknowledge(anomaly.key);
                }}
              >
                Acknowledge
              </button>
            {/if}
          </li>
        {/each}
      </ul>
      {#if store.anomalyError}
        <p class="fault" role="alert">The anomaly could not be acknowledged: {store.anomalyError}</p>
      {/if}
    </section>
  {/if}

  <PendingCatalogue {store} showIssues={false} />

  {#if relay}
    <section class="block" data-testid="relay-issues" aria-labelledby="relay-issues-title">
      <h2 id="relay-issues-title">Issues in the floor</h2>
      {#if relay.issues.length === 0}
        <p class="muted">None: every action of the floor is valid.</p>
      {:else}
        <p class="muted">These entries are not part of any catalogue until they are fixed.</p>
        <ul class="issues">
          {#each relay.issues as issue, i (i)}
            <li data-testid="relay-issue">
              <strong>{issue.name}</strong>
              {#if issue.line !== null}<span data-testid="relay-issue-line">line {issue.line}</span>{/if}:
              {issue.message}
            </li>
          {/each}
        </ul>
      {/if}
      {#if relay.notices.length > 0}
        <h3>Values a program reads</h3>
        <ul class="issues">
          {#each relay.notices as notice, i (i)}
            <li data-testid="relay-notice" data-kind={notice.kind}>
              <strong>{notice.action}</strong>, parameter {notice.param}: {notice.explanation}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="block" data-testid="relay-catalogue" aria-labelledby="relay-catalogue-title">
      <h2 id="relay-catalogue-title">Approved catalogue</h2>
      {#if relay.catalogue}
        <p class="muted">
          What <code>officina run</code> serves{relay.approved_at_ms !== null
            ? `, approved ${clock(relay.approved_at_ms, nowMs)}`
            : ""}.
        </p>
        <CatalogueTable catalogue={relay.catalogue} credentials={relay.credentials} />
      {:else}
        <p class="muted" data-testid="catalogue-none">Nothing is approved: <code>officina run</code> refuses every action.</p>
      {/if}
    </section>

    <section class="block" data-testid="relay-recent" aria-labelledby="relay-recent-title">
      <h2 id="relay-recent-title">Recent activity</h2>
      <RecentActivity recent={relay.recent} {nowMs} />
    </section>

    <section class="block" data-testid="relay-diagnostics" aria-labelledby="relay-diagnostics-title">
      <h2 id="relay-diagnostics-title">Journal</h2>
      <p class:fault={relay.journal_corrupt_lines > 0} data-testid="journal-corrupt">
        {relay.journal_corrupt_lines} unreadable journal {relay.journal_corrupt_lines === 1 ? "line" : "lines"} in the two
        days read.
      </p>
      <p class:fault={relay.gate_errors > 0} data-testid="gate-errors">
        {relay.gate_errors} gate {relay.gate_errors === 1 ? "error" : "errors"} in the last {hours(relay.gate_errors_window_ms)}.
      </p>
    </section>
  {/if}
</main>

<style>
  .relay-view {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 0 0 20px;
  }
  .block {
    margin: 0 20px;
    padding: 12px 18px;
    border-radius: 16px;
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
    overflow-wrap: anywhere;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
  .issues {
    margin: 0;
    padding-left: 18px;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .issues li {
    margin: 3px 0;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .tool {
    margin-top: 6px;
    min-height: var(--target-min);
    padding: 0 18px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .tool[aria-pressed="true"] {
    border-color: var(--color-stopped);
    background: var(--color-stopped-surface);
    color: var(--color-stopped-text);
  }
</style>
