<script lang="ts">
  // One `human` action's request in Needs you (AD-35): the command rebuilt
  // from the approved catalogue, one chip per argument, and Approve and
  // Reject that act on a pointer click only (D-28). Every text is rendered
  // as text: arguments, descriptions and outputs come from files a session
  // can write.
  import type { ApprovalView } from "../lib/generated/ApprovalView";
  import type { BoardStore } from "../lib/board.svelte";
  import { isPointerClick } from "../lib/relay.svelte";
  import { ago, duration } from "../lib/time";

  interface Props {
    approval: ApprovalView;
    store: BoardStore;
  }
  let { approval, store }: Props = $props();

  let error = $derived(store.approvalErrors[approval.id]);
  let busy = $derived(store.deciding[approval.id] === true);
  let dryRun = $derived(approval.dry_run);

  function decide(event: MouseEvent, approve: boolean): void {
    if (!isPointerClick(event)) return;
    void store.decideApproval(approval.id, approve);
  }

  /** Enter and Space never decide (D-28): the browser's click they would
   *  synthesise is stopped before it is made. */
  function noKeyDecides(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") event.preventDefault();
  }
</script>

<article class="approval" class:invalid={!approval.valid} data-testid="approval-card" data-id={approval.id}>
  <div class="head">
    <span class="project" data-testid="approval-project">{approval.project}</span>
    <span class="tag">{approval.valid ? "approval" : "invalid request"}</span>
  </div>
  <div class="action" data-testid="approval-action">{approval.action}</div>
  {#if approval.description}
    <p class="description" data-testid="approval-description">{approval.description}</p>
  {/if}

  {#if approval.valid}
    <ol class="argv" data-testid="argv" aria-label="The command, one argument per chip">
      {#each approval.argv as arg, i (i)}
        <li class="arg" data-testid="arg">{arg}</li>
      {/each}
    </ol>
    {#if dryRun}
      <section class="dry-run" data-testid="dry-run" aria-label="Dry run">
        <div class="dry-run-head">Dry run: {dryRun.check}</div>
        {#if dryRun.output !== null}
          <pre data-testid="dry-run-output">{dryRun.output}</pre>
        {:else}
          <p class="missing" data-testid="dry-run-missing">{dryRun.missing}</p>
        {/if}
        {#if dryRun.files_unchanged === true}
          <p class="unchanged" data-testid="files-unchanged">tracked and unignored files unchanged since the dry run</p>
        {:else if dryRun.files_unchanged === false}
          <p class="changed" data-testid="files-changed">
            tracked or unignored files changed since the dry run: the run will refuse this approval
          </p>
        {/if}
      </section>
    {/if}
  {:else}
    <p class="reason" data-testid="invalid-reason">
      Not approvable: {approval.invalid_reason}. Officina shows no command it cannot rebuild from the approved catalogue.
    </p>
  {/if}

  <div class="meta">
    <span data-testid="approval-session">{approval.session_name ?? "session unknown"}</span>
    <span>asked {ago(approval.requested_at_ms, store.nowMs)}</span>
    <span>expires in {duration(approval.expires_at_ms - store.nowMs)}</span>
  </div>

  {#if approval.valid}
    <div class="buttons">
      <button
        class="approve"
        data-testid="approve"
        disabled={busy}
        onclick={(event) => decide(event, true)}
        onkeydown={noKeyDecides}
      >
        Approve
      </button>
      <button
        class="reject"
        data-testid="reject"
        disabled={busy}
        onclick={(event) => decide(event, false)}
        onkeydown={noKeyDecides}
      >
        Reject
      </button>
    </div>
  {/if}
  {#if error}
    <p class="error" role="alert" data-testid="approval-error">{error}</p>
  {/if}
</article>

<style>
  .approval {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px 14px;
    background: var(--color-surface);
    border: 2px solid var(--color-attention);
    border-radius: var(--radius-card);
    min-width: 0;
  }
  .approval.invalid {
    border-color: var(--color-border);
    border-style: dashed;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .project {
    font-size: 13px;
    font-weight: 600;
    padding: 1px 10px;
    border-radius: var(--radius-pill);
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    overflow-wrap: anywhere;
  }
  .tag {
    margin-left: auto;
    font-size: 12px;
    font-weight: 700;
    padding: 1px 9px;
    border-radius: var(--radius-pill);
    background: var(--color-attention);
    color: var(--color-attention-text);
  }
  .invalid .tag {
    background: var(--color-border);
    color: var(--color-text);
  }
  .action {
    font-size: 16px;
    font-weight: 700;
    overflow-wrap: anywhere;
  }
  .description,
  .reason,
  .missing {
    margin: 0;
    font-size: 13.5px;
    color: var(--color-text-muted);
    overflow-wrap: anywhere;
  }
  .argv {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 2px 0;
    padding: 0;
    list-style: none;
  }
  .arg {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
    padding: 1px 7px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    /* A bidirectional control in one argument stays inside its chip. */
    unicode-bidi: isolate;
  }
  .dry-run {
    border-top: 1px solid var(--color-border);
    padding-top: 6px;
  }
  .dry-run-head {
    font-size: 12.5px;
    font-weight: 700;
    color: var(--color-text-muted);
  }
  pre {
    margin: 4px 0;
    max-height: 220px;
    overflow: auto;
    padding: 6px 8px;
    border-radius: 8px;
    background: var(--color-bg);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    unicode-bidi: isolate;
  }
  .unchanged,
  .changed {
    margin: 0;
    font-size: 12.5px;
  }
  .changed {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    font-size: 12.5px;
    color: var(--color-text-muted);
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  .buttons button {
    flex: 1;
    min-height: var(--target-min);
    border-radius: var(--radius-pill);
    font-weight: 700;
    cursor: pointer;
  }
  .approve {
    border: 0;
    background: var(--color-attention);
    color: var(--color-attention-text);
  }
  .reject {
    border: 1px solid var(--color-border);
    background: transparent;
  }
  .buttons button:disabled {
    cursor: progress;
    opacity: 0.6;
  }
  .error {
    margin: 0;
    font-size: 13px;
    color: var(--color-stopped-text);
    overflow-wrap: anywhere;
  }
</style>
