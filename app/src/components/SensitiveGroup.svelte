<script lang="ts">
  // One sensitive group of a package (AD-113, FR-11): its family, its
  // files, its hunks inline and the SHA-256 Approve sends. The hunks and
  // paths are worker text: rendered as text, every format character as a
  // visible escape (D-245), while the hash stays the one of the raw bytes.
  // Approve acts on a pointer click only (D-28): it lets a package's
  // sensitive change reach Import.
  import type { ReviewFile } from "../lib/generated/ReviewFile";
  import type { ReviewGroup } from "../lib/generated/ReviewGroup";
  import { escapeFormat } from "../lib/inert";
  import { isPointerClick } from "../lib/pointer";
  import { APPROVAL_REFUSAL_TEXT, GROUP_TEXT } from "../lib/workshop-text";
  import type { ApprovalRefusal } from "../lib/generated/ApprovalRefusal";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    group: ReviewGroup;
    files: ReviewFile[];
    approving: boolean;
    error: ApprovalRefusal | string | undefined;
    /** Approves the group: called on a pointer click only. */
    onApprove: () => void;
  }
  let { group, files, approving, error, onApprove }: Props = $props();

  const APPROVE_LABELS = { idle: "Approve", approving: "Approving", approved: "Approved" } as const;

  let paths = $derived(
    group.files.flatMap((index) => files.find((f) => f.index === index)?.paths ?? []).map(escapeFormat),
  );
  let enabled = $derived(group.approvable && !group.approved && !approving && group.hunks_sha256 !== null);
  let errorText = $derived(
    error === undefined ? null : error in APPROVAL_REFUSAL_TEXT ? APPROVAL_REFUSAL_TEXT[error as ApprovalRefusal] : error,
  );

  function clicked(event: MouseEvent): void {
    if (!isPointerClick(event) || !enabled) return;
    onApprove();
  }

  function noKeyApproves(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") event.preventDefault();
  }
</script>

<section class="group" data-testid="sensitive-group" data-group={group.key} data-approved={group.approved}>
  <h4>{GROUP_TEXT[group.group]}</h4>
  {#if paths.length > 0}
    <ul class="paths" aria-label="Files">
      {#each paths as path, i (i)}<li class="text" data-testid="group-path">{path}</li>{/each}
    </ul>
  {/if}
  {#if group.hunks !== null}
    <pre data-testid="group-hunks">{escapeFormat(group.hunks)}</pre>
  {:else}
    <p class="muted" data-testid="group-no-hunks">No hunk to show: {group.hunks_bytes === null ? "the change has no text" : `${group.hunks_bytes} bytes, too large to show`}.</p>
  {/if}
  {#if group.hunks_sha256}
    <p class="muted hash" data-testid="group-sha">SHA-256 <code>{group.hunks_sha256}</code></p>
  {/if}
  <div class="actions">
    <button type="button" data-testid="approve-group" disabled={!enabled} onclick={clicked} onkeydown={noKeyApproves}>
      <StableLabel labels={APPROVE_LABELS} current={group.approved ? "approved" : approving ? "approving" : "idle"} />
    </button>
    {#if !group.approvable && !group.approved}
      <span class="muted" data-testid="group-not-approvable">Too large to approve.</span>
    {/if}
  </div>
  {#if errorText}
    <p class="fault" role="alert" data-testid="approve-refused">{errorText}</p>
  {/if}
</section>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-surface);
  }
  h4 {
    margin: 0;
    font-size: 13px;
  }
  p {
    margin: 0;
    font-size: 13px;
  }
  .paths {
    margin: 0;
    padding-left: 18px;
    font-size: 12.5px;
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    unicode-bidi: isolate;
  }
  pre {
    margin: 0;
    max-height: 240px;
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
  .hash code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 11.5px;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  button {
    min-height: var(--target-min);
    padding: 0 14px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    color: var(--color-text);
    font-weight: 600;
    cursor: pointer;
  }
  button:disabled {
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
