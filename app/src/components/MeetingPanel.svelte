<script lang="ts">
  // The meeting (FR-5, AD-84, AD-93, D-111, D-528, D-564): the session
  // file's slot at the sessions area's right edge, one panel at a time,
  // never resizing what it overlays; Close does what Escape does and
  // returns the focus. It shows what the session expects, what it ran and
  // what blocked, the commits and the diff since it started and, for a
  // background session, its recent output; then how an answer would reach
  // it. Every session text, subject, path and log line renders as text.
  // The orchestrator's latest recap for the session shows under a neutral
  // "proposal" label (story 5.17, D-564): its texts are the orchestrator's,
  // rendered as text, and never reach the composer.
  import { onMount } from "svelte";
  import type { Board } from "../lib/generated/Board";
  import type { Card } from "../lib/generated/Card";
  import type { MeetingRun } from "../lib/generated/MeetingRun";
  import type { BoardStore } from "../lib/board.svelte";
  import { meeting } from "../lib/meeting.svelte";
  import { CHANGES_TEXT, DELIVERY_TEXT, runResultText } from "../lib/meeting-text";
  import { clock } from "../lib/time";
  import DiffView from "./DiffView.svelte";
  import AnswerComposer from "./AnswerComposer.svelte";

  interface Props {
    board: Board | null;
    store: BoardStore;
    /** Shows a run's journal entry in the Relay view. */
    onJournal: (run: MeetingRun) => void;
  }
  let { board, store, onJournal }: Props = $props();

  const titleId = $props.id();
  let view = $derived(meeting.view);
  let card = $derived<Card | null>(
    view && board ? (Object.values(board.columns).flat().find((c: Card) => c.key === view.key) ?? null) : null,
  );
  let changes = $derived(view?.changes ?? null);
  let recap = $derived(view?.recap?.content && "recap" in view.recap.content ? view.recap.content.recap : null);
  let logLines = $derived(view?.logs?.state === "shown" ? view.logs.text.replace(/\n$/, "").split("\n") : []);

  let panel: HTMLElement;
  let logsBody = $state<HTMLElement | null>(null);
  /** Where the sessions area starts in the viewport: the panel stands a
   *  gap under it, as the session file does (D-702). */
  let top = $state(0);

  function measure(): void {
    const area = document.querySelector("main");
    top = Math.max(0, area?.getBoundingClientRect().top ?? 0);
  }

  onMount(() => {
    measure();
    panel.querySelector<HTMLElement>("[data-testid=meeting-close]")?.focus();
    window.addEventListener("resize", measure);
    window.addEventListener("scroll", measure, { passive: true });
    return () => {
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", measure);
    };
  });

  function onKey(event: KeyboardEvent): void {
    if (event.key !== "Escape" || event.ctrlKey || event.altKey || event.metaKey) return;
    event.preventDefault();
    meeting.close();
  }

  function jumpToError(line: number): void {
    const found = logsBody?.querySelector<HTMLElement>(`[data-log-line="${line}"]`);
    found?.scrollIntoView({ block: "start" });
    found?.focus({ preventScroll: true });
  }
</script>

<svelte:window onkeydown={onKey} />

<div
  class="meeting"
  role="dialog"
  aria-labelledby={titleId}
  data-testid="meeting-panel"
  data-key={meeting.key}
  bind:this={panel}
  style:--meeting-top="{top}px"
>
  <header>
    <h2 id={titleId}>{view ? view.name : "Meeting"}</h2>
    <button type="button" class="close" data-testid="meeting-close" onclick={() => meeting.close()}>Close</button>
  </header>

  {#if meeting.error}
    <p class="error" role="alert" data-testid="meeting-error">{meeting.error}</p>
  {:else if !view}
    <p class="muted" data-testid="meeting-loading">Reading what this session did...</p>
  {/if}

  {#if view}
    <section data-testid="meeting-decision" aria-label="Decision expected">
      <h3>Decision expected</h3>
      <dl>
        {#if view.decision.waiting_for}
          <dt>Waiting for</dt>
          <dd data-testid="meeting-waiting-for">{view.decision.waiting_for}</dd>
        {/if}
        {#if view.decision.notification}
          <dt>Claude Code said</dt>
          <dd class="text" data-testid="meeting-notification">{view.decision.notification}</dd>
        {/if}
        {#if view.decision.answer_end}
          <dt>End of its last answer</dt>
          <dd class="text" data-testid="meeting-answer-end">{view.decision.answer_end}</dd>
        {/if}
        {#if view.decision.last_prompt}
          <dt>Your last prompt</dt>
          <dd class="text" data-testid="meeting-last-prompt">{view.decision.last_prompt}</dd>
        {/if}
      </dl>
    </section>

    <section data-testid="meeting-delivery" data-delivery={view.delivery} aria-label="Answer">
      <h3>Answer</h3>
      <p>{DELIVERY_TEXT[view.delivery]}</p>
      {#if view.delivery === "focus_only"}
        {#if card}
          <button type="button" class="action" data-testid="meeting-focus" onclick={() => store.focus(card!)}>Focus</button>
        {/if}
      {:else}
        <AnswerComposer {view} />
      {/if}
    </section>

    <section data-testid="meeting-runs" aria-label="Runs and approvals">
      <h3>Runs and approvals</h3>
      {#if view.runs.length === 0 && view.approvals.length === 0}
        <p class="muted">No relay run or approval since it started.</p>
      {/if}
      <ul class="rows">
        {#each view.runs as run, i (i)}
          <li data-testid="meeting-run" data-result={run.result}>
            <span class="when">{clock(run.at_ms, store.nowMs)}</span>
            <span class="what">{run.action}</span>
            <span class="how">{runResultText(run)}</span>
            {#if run.result === "failed" && run.run_id}
              <button type="button" class="link" data-testid="meeting-journal" title="journal/{run.journal_file}" onclick={() => onJournal(run)}>Journal entry</button>
            {/if}
          </li>
        {/each}
        {#each view.approvals as approval (approval.id)}
          <li data-testid="meeting-approval">
            <span class="when">{clock(approval.requested_at_ms, store.nowMs)}</span>
            <span class="what">{approval.action}</span>
            <span class="how">waits for an approval (class {approval.class})</span>
          </li>
        {/each}
      </ul>
    </section>

    {#if recap && view.recap}
      <section data-testid="meeting-recap" aria-label="Recap">
        <h3>Recap</h3>
        <p class="byline">
          <span class="chip" data-testid="meeting-recap-label">proposal</span>
          <span class="muted">from the orchestrator, {clock(view.recap.received_at_ms ?? view.recap.written_at_ms, store.nowMs)}</span>
        </p>
        <dl>
          <dt>Done</dt>
          <dd class="text" data-testid="meeting-recap-done">{recap.done}</dd>
          <dt>Blocking</dt>
          <dd class="text" data-testid="meeting-recap-blocking">{recap.blocking}</dd>
          <dt>Decision expected</dt>
          <dd class="text" data-testid="meeting-recap-decision">{recap.decision_expected}</dd>
        </dl>
      </section>
    {/if}

    <section data-testid="meeting-changes" data-state={changes?.state} aria-label="Changes">
      <h3>Changes</h3>
      {#if changes?.state === "read"}
        {#if changes.commits.length === 0}
          <p class="muted">No commit since it started.</p>
        {/if}
        <ol class="commits">
          {#each changes.commits as commit (commit.hash)}
            <li data-testid="meeting-commit">
              <span class="hash">{commit.hash.slice(0, 8)}</span>
              <span class="subject">{commit.subject}</span>
              <ul class="files">
                {#each commit.files as file, i (i)}<li>{file}</li>{/each}
              </ul>
            </li>
          {/each}
        </ol>
        {#if changes.diff_scope_note}<p class="muted" data-testid="meeting-diff-note">{changes.diff_scope_note}</p>{/if}
        <DiffView diff={changes.diff} />
        {#if changes.untracked.length > 0}
          <h4>Untracked files, not read</h4>
          <ul class="files" data-testid="meeting-untracked">
            {#each changes.untracked as file, i (i)}
              <li>{file.path}{#if file.bytes !== null}<span class="muted"> ({file.bytes} bytes)</span>{/if}</li>
            {/each}
          </ul>
          {#if changes.untracked_more}<p class="muted">More untracked files are not listed.</p>{/if}
        {/if}
      {:else if changes?.state === "unavailable"}
        <p class="muted" data-testid="meeting-changes-note">Git could not be read: {changes.reason}</p>
      {:else if changes}
        <p class="muted" data-testid="meeting-changes-note">{CHANGES_TEXT[changes.state]}</p>
      {/if}
    </section>

    {#if view.logs}
      <section data-testid="meeting-logs" aria-label="Recent output">
        <h3>Recent output</h3>
        {#if view.logs.state === "shown"}
          {@const logs = view.logs}
          {#if logs.first_error_line !== null}
            <button type="button" class="link" data-testid="meeting-first-error" onclick={() => jumpToError(logs.first_error_line!)}>
              First error line ({logs.first_error_line})
            </button>
          {/if}
          {#if logs.dropped_bytes > 0}<p class="muted">Its start is not shown.</p>{/if}
          <pre class="logs" data-testid="meeting-logs-text" bind:this={logsBody}>{#each logLines as line, i (i)}<span class="line" data-log-line={i + 1} tabindex="-1">{line}</span>{/each}</pre>
        {:else}
          <p class="muted" data-testid="meeting-logs-note">No output to show: {view.logs.reason}</p>
        {/if}
      </section>
    {/if}
  {/if}
</div>

<style>
  /* The session file's slot (D-111, D-702): a surface standing in the
     sessions area at its right edge, a gap under the header. */
  .meeting {
    position: fixed;
    top: calc(var(--meeting-top) + var(--file-gap));
    right: var(--file-inset);
    bottom: var(--file-inset);
    z-index: 2;
    width: var(--file-width);
    max-width: calc(100vw - 2 * var(--file-inset));
    overflow: auto;
    padding: 16px 20px 20px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-surface);
    box-shadow: 0 6px 16px -4px var(--color-shadow);
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }
  h2 {
    flex: 1;
    min-width: 0;
    margin: 8px 0 0;
    font-size: 18px;
    overflow-wrap: anywhere;
  }
  h3 {
    margin: 18px 0 6px;
    font-size: 14px;
  }
  h4 {
    margin: 10px 0 4px;
    font-size: 13px;
  }
  .close,
  .action {
    min-height: var(--target-min);
    padding: 0 14px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .close {
    flex: none;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 14px;
    margin: 0;
  }
  dt {
    color: var(--color-text-muted);
    font-size: 13px;
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .text {
    white-space: pre-wrap;
  }
  /* A bidirectional control in the orchestrator's text stays inside it. */
  [data-testid="meeting-recap"] .text {
    unicode-bidi: isolate;
  }
  .byline {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
  }
  .chip {
    font-size: 12.5px;
    padding: 1px 7px;
    border-radius: var(--radius-surface);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
  }
  p {
    margin: 0 0 6px;
  }
  .muted {
    color: var(--color-text-muted);
    font-size: 13px;
  }
  .error {
    margin-top: 12px;
    color: var(--color-danger);
  }
  .rows,
  .commits,
  .files {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .rows li {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 10px;
    padding: 2px 0;
  }
  .when,
  .hash {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
    color: var(--color-text-muted);
  }
  .what,
  .subject {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .commits > li {
    padding: 4px 0;
  }
  .files li {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
    overflow-wrap: anywhere;
  }
  .link {
    border: 0;
    background: none;
    padding: 0;
    min-height: 32px;
    color: var(--color-working);
    font-weight: 600;
    cursor: pointer;
    text-decoration: underline;
  }
  .logs {
    margin: 0;
    max-height: 360px;
    overflow: auto;
    padding: 8px 10px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12px;
    line-height: 1.4;
  }
  .line {
    display: block;
    min-height: 1.4em;
    white-space: pre;
    outline-offset: -2px;
  }
</style>
