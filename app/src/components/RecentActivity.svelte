<script lang="ts">
  // The relay's recent activity (AD-35): the last entries of the journal,
  // runs paired by Rust, newest first. Parameters, refusal details and
  // command excerpts came from sessions: they are rendered as text only.
  import { activityRow } from "../lib/relay-text";
  import type { RecentEntry } from "../lib/generated/RecentEntry";
  import { clock, runDuration } from "../lib/time";

  interface Props {
    recent: RecentEntry[];
    nowMs: number;
  }
  let { recent, nowMs }: Props = $props();

  let rows = $derived(recent.map((entry) => ({ entry, row: activityRow(entry) })));
</script>

{#if rows.length === 0}
  <p class="muted" data-testid="recent-empty">Nothing in the journal of the last two days.</p>
{:else}
  <table class="recent" data-testid="recent-table">
    <thead>
      <tr>
        <th scope="col">Time</th>
        <th scope="col">Project</th>
        <th scope="col">Action</th>
        <th scope="col">Parameters</th>
        <th scope="col">Outcome</th>
        <th scope="col">Duration</th>
      </tr>
    </thead>
    <tbody>
      {#each rows as { entry, row }, i (i)}
        <tr
          data-testid="recent-entry"
          data-kind={entry.event.kind}
          data-state={entry.event.kind === "run" ? entry.event.state : undefined}
          class={row.tone}
        >
          <td>{clock(entry.ts, nowMs)}</td>
          <td>{row.project}</td>
          <td>{row.action}</td>
          <td>
            {#each row.params as [key, value] (key)}<code class="chip">{key}={value}</code>{/each}
          </td>
          <td>
            <span class="outcome" data-testid="recent-outcome">{row.outcome}</span>
            {#if row.detail}<div class="detail" data-testid="recent-detail">{row.detail}</div>{/if}
          </td>
          <td>{row.durationMs !== null ? runDuration(row.durationMs) : ""}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  .recent {
    width: 100%;
    border-collapse: collapse;
    font-size: 13.5px;
  }
  th {
    text-align: left;
    font-weight: 600;
    color: var(--color-text-muted);
    border-bottom: 1px solid var(--color-border);
    padding: 4px 8px;
  }
  td {
    vertical-align: top;
    padding: 5px 8px;
    border-bottom: 1px solid var(--color-border);
    overflow-wrap: anywhere;
  }
  tr.fault .outcome,
  tr.abandoned .outcome {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
  tr.ok .outcome {
    color: var(--color-project-green);
    font-weight: 600;
  }
  .detail {
    color: var(--color-text-muted);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12px;
    white-space: pre-wrap;
  }
  .muted {
    color: var(--color-text-muted);
    font-size: 13px;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .chip {
    display: inline-block;
    margin: 1px 4px 1px 0;
    padding: 1px 6px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    background: var(--color-bg);
  }
</style>
