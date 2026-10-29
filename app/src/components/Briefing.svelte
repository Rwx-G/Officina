<script lang="ts">
  // The briefing (AD-83, FR-1): what changed since the cursor, then what
  // waits for the author and what is open, each section with its count and,
  // when the bound left items out, how many. Core holds the 32 KiB bound and
  // its drop order: this component shows `dropped`, it never cuts a list
  // itself. Session names are untrusted and rendered as text.
  import type { Briefing } from "../lib/generated/Briefing";
  import type { Section } from "../lib/generated/Section";
  import {
    PHASE_TEXT,
    RUN_KIND_TEXT,
    OUTCOME_TEXT,
    SUBJECT_STATUS_TEXT,
    WAITING_COLUMN_TEXT,
    changeText,
    proposalKindText,
    sinceText,
  } from "../lib/orchestrator-text";
  import { clock } from "../lib/time";

  interface Props {
    briefing: Briefing;
    nowMs: number;
  }
  let { briefing, nowMs }: Props = $props();

  function at(atMs: number | null): string {
    return atMs === null ? "time unknown" : clock(atMs, nowMs);
  }

  function changeSubject(item: Briefing["changed"]["items"][number]): string {
    switch (item.kind) {
      case "run_finished":
        return `${item.action} in ${item.project}`;
      case "started_waiting":
        return item.name;
      case "subject_status":
        return item.name;
      case "order_launched":
        return item.name;
      case "proposal_received":
        return `${proposalKindText(item.proposal)} proposal`;
    }
  }

  function counted(section: Section<unknown>): string {
    return String(section.count);
  }
</script>

{#snippet dropped(name: string, section: Section<unknown>)}
  {#if section.dropped > 0}
    <p class="muted" data-testid="briefing-dropped-{name}">
      {section.dropped} older {section.dropped === 1 ? "item" : "items"} left out to keep the briefing short.
    </p>
  {/if}
{/snippet}

<section class="block" data-testid="briefing" aria-labelledby="briefing-title">
  <h2 id="briefing-title">Briefing</h2>
  <p class="muted" data-testid="briefing-since">{sinceText(briefing.cursor, nowMs)}</p>

  <h3 data-testid="briefing-section-changed">What changed <span class="count">{counted(briefing.changed)}</span></h3>
  {#if briefing.changed.count === 0}
    <p class="muted">Nothing.</p>
  {:else}
    <ul>
      {#each briefing.changed.items as item, i (i)}
        <li data-testid="briefing-change" data-kind={item.kind}>
          <span class="time">{clock(item.at_ms, nowMs)}</span>
          <strong>{changeSubject(item)}</strong>
          {changeText(item)}
        </li>
      {/each}
    </ul>
    {@render dropped("changed", briefing.changed)}
  {/if}

  <h3 data-testid="briefing-section-waiting">Waiting for you <span class="count">{counted(briefing.waiting)}</span></h3>
  {#if briefing.waiting.count === 0}
    <p class="muted">No session waits for you.</p>
  {:else}
    <ul>
      {#each briefing.waiting.items as item (item.key)}
        <li data-testid="briefing-waiting">
          <span class="time">{at(item.at_ms)}</span>
          <strong>{item.name}</strong>
          {WAITING_COLUMN_TEXT[item.column]}, {item.project}{item.subject ? `, ${item.subject.name}` : ""}{item.waiting_for
            ? `: ${item.waiting_for}`
            : ""}
        </li>
      {/each}
    </ul>
    {@render dropped("waiting", briefing.waiting)}
  {/if}

  <h3 data-testid="briefing-section-approvals">Pending approvals <span class="count">{counted(briefing.approvals)}</span></h3>
  {#if briefing.approvals.count === 0}
    <p class="muted">None.</p>
  {:else}
    <ul>
      {#each briefing.approvals.items as item (item.id)}
        <li data-testid="briefing-approval">
          <span class="time">{clock(item.at_ms, nowMs)}</span>
          <strong>{item.action}</strong> in {item.project}{item.session_name !== null ? `, for ${item.session_name}` : ""}{item.valid
            ? ""
            : " (invalid)"}
        </li>
      {/each}
    </ul>
    {@render dropped("approvals", briefing.approvals)}
  {/if}

  <h3 data-testid="briefing-section-subjects">Open subjects <span class="count">{counted(briefing.subjects)}</span></h3>
  {#if briefing.subjects.count === 0}
    <p class="muted">None.</p>
  {:else}
    <ul>
      {#each briefing.subjects.items as item (item.id)}
        <li data-testid="briefing-subject">
          <strong>{item.name}</strong>
          {item.project}, {SUBJECT_STATUS_TEXT[item.status]}
          {#if item.sessions.length > 0}
            <ul class="sessions">
              {#each item.sessions as session (session.session_id)}
                <li data-testid="briefing-subject-session">
                  {session.name ?? session.session_id}: {PHASE_TEXT[session.phase]}
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
    {@render dropped("subjects", briefing.subjects)}
  {/if}

  <h3 data-testid="briefing-section-triage">To sort <span class="count">{counted(briefing.triage)}</span></h3>
  {#if briefing.triage.count === 0}
    <p class="muted">Nothing to sort.</p>
  {:else}
    <ul>
      {#each briefing.triage.items as item (item.key)}
        <li data-testid="briefing-triage">
          <span class="time">{at(item.at_ms)}</span>
          <strong>{item.subject?.name ?? item.session_name ?? item.key}</strong>
          {item.project}
        </li>
      {/each}
    </ul>
    {@render dropped("triage", briefing.triage)}
  {/if}

  <h3 data-testid="briefing-section-runs">Failed and refused runs <span class="count">{counted(briefing.runs)}</span></h3>
  {#if briefing.runs.count === 0}
    <p class="muted">None in the relay's recent activity.</p>
  {:else}
    <ul>
      {#each briefing.runs.items as item, i (i)}
        <li data-testid="briefing-run" data-kind={item.kind}>
          <span class="time">{clock(item.at_ms, nowMs)}</span>
          <strong>{item.action ?? "a command"}</strong>
          {RUN_KIND_TEXT[item.kind]}{item.project ? ` in ${item.project}` : ""}{item.exit_code !== null
            ? `, exit ${item.exit_code}`
            : ""}{item.reason ? ` (${item.reason})` : ""}
        </li>
      {/each}
    </ul>
    {@render dropped("runs", briefing.runs)}
  {/if}

  <h3 data-testid="briefing-section-work_orders">Work orders <span class="count">{counted(briefing.work_orders)}</span></h3>
  {#if briefing.work_orders.count === 0}
    <p class="muted">None launched.</p>
  {:else}
    <ul>
      {#each briefing.work_orders.items as item (item.id)}
        <li data-testid="briefing-order">
          <span class="time">{clock(item.at_ms, nowMs)}</span>
          <strong>{item.name}</strong>
          {item.project}, {OUTCOME_TEXT[item.outcome]}{item.session
            ? `; ${item.session.name ?? item.session.session_id} ${PHASE_TEXT[item.session.phase]}`
            : ""}
        </li>
      {/each}
    </ul>
    {@render dropped("work_orders", briefing.work_orders)}
  {/if}

  <h3 data-testid="briefing-section-proposals">
    Proposals pending <span class="count">{counted(briefing.proposals)}</span>
  </h3>
  {#if briefing.proposals.count === 0}
    <p class="muted">None.</p>
  {:else}
    <ul>
      {#each briefing.proposals.items as item (item.id)}
        <li data-testid="briefing-proposal">
          <span class="time">{clock(item.received_at_ms ?? item.written_at_ms, nowMs)}</span>
          <strong>{proposalKindText(item.kind)}</strong>, listed under Proposals
        </li>
      {/each}
    </ul>
    {@render dropped("proposals", briefing.proposals)}
  {/if}
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
    margin: 12px 0 4px;
    font-size: 14px;
  }
  .count {
    margin-left: 4px;
    color: var(--color-text-muted);
    font-weight: 600;
  }
  p {
    margin: 4px 0;
    font-size: 13.5px;
  }
  ul {
    margin: 0;
    padding-left: 18px;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  li {
    margin: 3px 0;
  }
  .sessions {
    color: var(--color-text-muted);
  }
  .time {
    margin-right: 6px;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .muted {
    color: var(--color-text-muted);
  }
</style>
