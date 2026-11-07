<script lang="ts">
  // The per-template table (AD-114, FR-11, FR-12 as amended, D-222): each
  // template's runs and what became of them, a run with no change and a
  // failed run counted apart from the author's verdicts; Acceptance is
  // imported over decided. FR-11's notices, the week's Workshop share and
  // its environment failures (D-214, D-217).
  import type { EnvironmentFailures } from "../lib/generated/EnvironmentFailures";
  import type { TemplateRow } from "../lib/generated/TemplateRow";
  import type { WorkshopShare } from "../lib/generated/WorkshopShare";
  import { DISCARD_REASON_TEXT, GROUP_TEXT } from "../lib/workshop-text";

  interface Props {
    templates: TemplateRow[];
    share: WorkshopShare;
    failures: EnvironmentFailures;
  }
  let { templates, share, failures }: Props = $props();

  function percent(part: number, whole: number): string {
    return whole === 0 ? "none yet" : `${Math.round((100 * part) / whole)} %`;
  }

  let launches = $derived(share.workshop + share.host);
</script>

<div class="part" data-testid="template-table">
  <h3>Templates</h3>
  {#if templates.length === 0}
    <p class="muted" data-testid="templates-none">No workshop run recorded.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Template</th><th>Runs</th><th>No change</th><th>Failed</th><th>Quarantined</th><th>Imported</th><th>Discarded</th><th
            >Acceptance</th
          >
        </tr>
      </thead>
      <tbody>
        {#each templates as row (row.template)}
          <tr data-testid="template-row" data-template={row.template}>
            <td>{row.template}</td>
            <td data-testid="template-runs">{row.runs}</td>
            <td data-testid="template-no-change">{row.no_change}</td>
            <td data-testid="template-failed">{row.failed}</td>
            <td data-testid="template-quarantined">{row.quarantined}</td>
            <td data-testid="template-imported">{row.imported}</td>
            <td data-testid="template-discarded">
              {#each row.discarded.filter((d) => d.count > 0) as discard, i (discard.reason)}
                {i > 0 ? ", " : ""}{discard.count} <span class="muted">{DISCARD_REASON_TEXT[discard.reason].toLowerCase()}</span>
              {:else}
                0
              {/each}
            </td>
            <td data-testid="template-acceptance">{row.imported} of {row.decided}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#each templates.flatMap((row) => row.notices) as notice (`${notice.template}:${notice.group}`)}
      <p class="notice" data-testid="template-notice">
        {notice.template}: {GROUP_TEXT[notice.group]} appeared in {notice.packages} of the {notice.verdicts} packages you decided.
        Keep the group, or narrow the template.
      </p>
    {/each}
  {/if}
  <p data-testid="workshop-share">
    Workshop share, week {share.week}: {share.workshop} of {launches} launches, {percent(share.workshop, launches)}.
  </p>
  <p data-testid="environment-failures">
    Environment failures, week {failures.week}: {failures.failures} of {failures.runs} runs, {percent(failures.failures, failures.runs)}.
  </p>
</div>

<style>
  h3 {
    margin: 8px 0 4px;
    font-size: 14px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th,
  td {
    padding: 4px 6px;
    text-align: left;
    vertical-align: top;
    border-top: 1px solid var(--color-border);
    font-variant-numeric: tabular-nums;
  }
  th {
    font-weight: 600;
    color: var(--color-text-muted);
  }
  p {
    margin: 4px 0;
    font-size: 13px;
  }
  .notice {
    font-weight: 600;
  }
  .muted {
    color: var(--color-text-muted);
  }
</style>
