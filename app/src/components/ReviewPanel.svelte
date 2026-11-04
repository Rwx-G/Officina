<script lang="ts">
  // The review panel (AD-122, FR-10 to FR-12): a workshop run's package in
  // the meeting's side slot (AD-84), 640 px wide, one panel at a time. It
  // shows the report, the facts, the commits, the diff and one section per
  // sensitive group, then Import and Discard. Every run text renders as
  // text with each format character as a visible escape (D-245): the
  // report, commit subjects and authors, paths, hosts, hunks and errors.
  // Approve and Import act on a pointer click only (D-28): each lets the
  // worker's change into the repository. Discard only reduces, so its
  // three reasons take the keyboard too. A quarantined package offers
  // Discard only; a run with nothing to review offers nothing.
  import { onMount } from "svelte";
  import type { DiffView as Diff } from "../lib/generated/DiffView";
  import { escapeFormat } from "../lib/inert";
  import { isPointerClick } from "../lib/pointer";
  import { clock, duration } from "../lib/time";
  import { workshop } from "../lib/workshop.svelte";
  import {
    AUTHOR_DISCARD_REASONS,
    DISCARD_REASON_TEXT,
    DISCARD_REFUSAL_TEXT,
    IMPORT_REFUSAL_TEXT,
    NOT_READY_TEXT,
    QUARANTINE_TEXT,
    REVOKE_ADVICE,
    RUN_STATE_TEXT,
    dollars,
  } from "../lib/workshop-text";
  import DiffView from "./DiffView.svelte";
  import SensitiveGroup from "./SensitiveGroup.svelte";

  interface Props {
    nowMs: number;
  }
  let { nowMs }: Props = $props();

  const titleId = $props.id();
  let review = $derived(workshop.review);
  let quarantined = $derived(review?.state === "quarantined");
  let inReview = $derived(review?.state === "review");
  let decided = $derived(review !== null && !inReview && !quarantined);
  let diff = $derived<Diff | null>(
    review?.diff
      ? {
          ...review.diff,
          text: escapeFormat(review.diff.text),
          files: review.diff.files.map((file) => ({ ...file, path: escapeFormat(file.path) })),
        }
      : null,
  );
  let importable = $derived(review !== null && inReview && review.readiness.ready && review.tip !== null);

  let panel: HTMLElement;
  let top = $state(0);

  function measure(): void {
    const area = document.querySelector("main");
    top = Math.max(0, area?.getBoundingClientRect().top ?? 0);
  }

  onMount(() => {
    measure();
    panel.querySelector<HTMLElement>("[data-testid=review-close]")?.focus();
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
    workshop.close();
  }

  function importClicked(event: MouseEvent): void {
    if (!isPointerClick(event) || !importable || workshop.importing) return;
    void workshop.importPackage();
  }

  function noKeyImports(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") event.preventDefault();
  }

  const short = (hash: string): string => hash.slice(0, 12);
</script>

<svelte:window onkeydown={onKey} />

<div
  class="review"
  role="dialog"
  aria-labelledby={titleId}
  data-testid="review-panel"
  data-run={workshop.runId}
  data-state={review?.state}
  bind:this={panel}
  style:--review-top="{top}px"
>
  <header>
    <h2 id={titleId}>{review ? `${review.template} in ${review.project}` : "Review"}</h2>
    <button type="button" class="close" data-testid="review-close" onclick={() => workshop.close()}>Close</button>
  </header>

  {#if workshop.error}
    <p class="fault" role="alert" data-testid="review-error">{escapeFormat(workshop.error)}</p>
  {/if}
  {#if !review}
    {#if !workshop.error}<p class="muted" data-testid="review-loading">Reading the run's package...</p>{/if}
  {:else}
    <p class="state" data-testid="review-state">
      {RUN_STATE_TEXT[review.state]}, from <code>{escapeFormat(review.base_branch)}</code> @
      <code>{short(review.base_commit)}</code>{#if review.tip}, tip <code data-testid="review-tip">{short(review.tip)}</code>{/if}
    </p>

    {#if review.nothing_to_review}
      <p class="nothing" data-testid="review-nothing">
        Nothing to review: the work branch has no commit past {escapeFormat(review.base_branch)} @ {short(review.base_commit)}.
      </p>
    {/if}

    {#if quarantined && review.quarantine_cause}
      <section class="quarantine" data-testid="review-quarantine" data-cause={review.quarantine_cause}>
        <h3>Quarantined</h3>
        <p>{QUARANTINE_TEXT[review.quarantine_cause]}</p>
        {#if review.quarantine_cause === "credential_found"}
          <p class="fault" data-testid="review-revoke">
            {REVOKE_ADVICE}{#if review.facts.credential_locations !== null}
              Found in {review.facts.credential_locations} place{review.facts.credential_locations === 1 ? "" : "s"}.{/if}
          </p>
          {#if review.facts.credential_places.length > 0}
            <ul class="list">
              {#each review.facts.credential_places as place, i (i)}<li class="text">{escapeFormat(place)}</li>{/each}
            </ul>
          {/if}
        {/if}
      </section>
    {/if}

    <section aria-label="Report">
      <h3>Report</h3>
      {#if review.report_tail !== null}
        <pre data-testid="review-report">{escapeFormat(review.report_tail)}</pre>
      {:else}
        <p class="muted">The worker left no report.</p>
      {/if}
    </section>

    <section aria-label="Facts" data-testid="review-facts">
      <h3>Facts</h3>
      <dl>
        {#if review.facts.duration_ms !== null}<dt>Ran for</dt><dd>{duration(review.facts.duration_ms)}</dd>{/if}
        {#if review.facts.cost_usd_micros !== null}<dt>Cost</dt><dd>{dollars(review.facts.cost_usd_micros / 10_000)}</dd>{/if}
        {#if review.facts.turns !== null}<dt>Turns</dt><dd>{review.facts.turns}</dd>{/if}
        {#if review.facts.exit_code !== null}<dt>Exit code</dt><dd>{review.facts.exit_code}</dd>{/if}
        {#if review.facts.commits_past_base !== null}<dt>Commits</dt><dd>{review.facts.commits_past_base}</dd>{/if}
        {#if review.facts.files_changed !== null}
          <dt>Files</dt>
          <dd>{review.facts.files_changed} changed, +{review.facts.insertions ?? 0} -{review.facts.deletions ?? 0}</dd>
        {/if}
        {#if review.facts.clock_skew_ms !== null}<dt>Engine clock</dt><dd>{review.facts.clock_skew_ms} ms off</dd>{/if}
        {#if review.facts.collect_failed !== null}<dt>Collection</dt><dd>failed ({review.facts.collect_failed})</dd>{/if}
        <dt>Hosts reached</dt>
        <dd data-testid="review-allowed">
          {#if review.facts.allowed.length === 0}none{:else}
            {#each review.facts.allowed as [host, count], i (i)}<span class="text host">{escapeFormat(host)} ({count})</span>{/each}
          {/if}
        </dd>
        <dt>Refused</dt>
        <dd data-testid="review-refused">
          {review.facts.refused}{#each review.facts.refusals as refusal, i (i)}<span class="text host"
              >{escapeFormat(refusal.host)}:{refusal.port} ({refusal.cause})</span
            >{/each}
        </dd>
      </dl>
    </section>

    {#if review.commits.length > 0}
      <section aria-label="Commits">
        <h3>Commits</h3>
        <ul class="list" data-testid="review-commits">
          {#each review.commits as commit (commit.hash)}
            <li data-testid="review-commit">
              <code>{short(commit.hash)}</code>
              <span class="text">{escapeFormat(commit.subject)}</span>
              <span class="muted text">{escapeFormat(commit.author)}, {clock(commit.at_ms, nowMs)}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if diff}
      <section aria-label="Diff">
        <h3>Diff</h3>
        <DiffView {diff} />
      </section>
    {/if}

    {#if review.groups.length > 0}
      <section aria-label="Sensitive groups" class="groups">
        <h3>Sensitive changes</h3>
        {#each review.groups as group (group.key)}
          <SensitiveGroup
            {group}
            files={review.files}
            approving={workshop.approving[group.key] === true}
            error={workshop.approvalErrors[group.key]}
            onApprove={() => void workshop.approve(group.key)}
          />
        {/each}
      </section>
    {/if}

    {#if inReview}
      <section class="decide" aria-label="Import" data-testid="review-import-section">
        <h3>Import</h3>
        <p class="muted">
          Creates a local branch at tip <code>{review.tip ? short(review.tip) : "unknown"}</code> from
          <code>{escapeFormat(review.base_branch)}</code>; nothing is merged or pushed.
        </p>
        <button type="button" class="primary" data-testid="review-import" disabled={!importable || workshop.importing} onclick={importClicked} onkeydown={noKeyImports}>
          Import
        </button>
        {#if !review.readiness.ready}
          <ul class="list" data-testid="review-not-ready">
            {#each review.readiness.reasons as reason, i (i)}<li>{NOT_READY_TEXT[reason.cause]}</li>{/each}
          </ul>
        {/if}
      </section>
    {/if}
    {#if workshop.imported?.outcome === "imported"}
      <p data-testid="review-imported">Imported as <code class="text">{escapeFormat(workshop.imported.detail)}</code>.</p>
    {:else if workshop.imported?.outcome === "refused"}
      <p class="fault" role="alert" data-testid="review-import-refused">{IMPORT_REFUSAL_TEXT[workshop.imported.detail]}</p>
    {/if}

    {#if inReview || quarantined}
      <section class="decide" aria-label="Discard" data-testid="review-discard-section">
        <h3>Discard</h3>
        <div class="reasons">
          {#each AUTHOR_DISCARD_REASONS as reason (reason)}
            <button
              type="button"
              data-testid="review-discard"
              data-reason={reason}
              disabled={workshop.discarding}
              onclick={() => void workshop.discard(reason)}>{DISCARD_REASON_TEXT[reason]}</button
            >
          {/each}
        </div>
      </section>
    {/if}
    {#if workshop.discarded?.outcome === "discarded"}
      <p data-testid="review-discarded">Discarded.</p>
    {:else if workshop.discarded?.outcome === "refused"}
      <p class="fault" role="alert" data-testid="review-discard-refused">{DISCARD_REFUSAL_TEXT[workshop.discarded.detail]}</p>
    {/if}
    {#if decided && !review.nothing_to_review}
      <p class="muted" data-testid="review-decided">This run is {RUN_STATE_TEXT[review.state]}: nothing left to decide.</p>
    {/if}
  {/if}
</div>

<style>
  /* The meeting's slot (AD-84, D-111, D-702), 640 px wide (AD-122). */
  .review {
    position: fixed;
    top: calc(var(--review-top) + var(--file-gap));
    right: var(--file-inset);
    bottom: var(--file-inset);
    z-index: 2;
    width: 640px;
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
  p {
    margin: 4px 0;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  pre {
    margin: 0;
    max-height: 260px;
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
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    unicode-bidi: isolate;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 14px;
    margin: 0;
    font-size: 13px;
  }
  dt {
    color: var(--color-text-muted);
  }
  dd {
    margin: 0;
    min-width: 0;
  }
  .host {
    display: inline-block;
    margin-right: 10px;
  }
  .list {
    margin: 0;
    padding-left: 18px;
    font-size: 13px;
  }
  .list li {
    margin: 2px 0;
  }
  .groups {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .reasons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  button {
    min-height: var(--target-min);
    padding: 0 14px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    font-weight: 600;
    cursor: pointer;
  }
  button.primary {
    border-color: var(--color-text);
    padding: 0 22px;
    font-weight: 700;
  }
  button:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .close {
    flex: none;
  }
  .nothing {
    font-weight: 600;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
