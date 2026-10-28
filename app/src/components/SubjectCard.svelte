<script lang="ts">
  // One open subject (FR-6, journey 1): its sessions with their live state,
  // and when none runs, its last prompt and the end of its last answer; the
  // author renames, merges or archives it here (AD-48). A refusal shows its
  // reason and nothing else changes. Subject and session text renders as
  // text.
  import type { SubjectView } from "../lib/generated/SubjectView";
  import { isLive, subjectTarget, type RegistryStore } from "../lib/registry.svelte";
  import { SUBJECT_STATUS_TEXT } from "../lib/registry-text";
  import SessionRow from "./SessionRow.svelte";

  interface Props {
    subject: SubjectView;
    store: RegistryStore;
    /** The open subjects it may be merged into. */
    others: SubjectView[];
  }
  let { subject, store, others }: Props = $props();

  type Mode = "idle" | "rename" | "merge" | "archive";
  let mode = $state<Mode>("idle");
  let newName = $state("");
  let mergeTarget = $state<number | null>(null);
  let routine = $state(false);

  let target = $derived(subjectTarget(subject.id));
  let busy = $derived(store.inFlight[target] ?? false);
  let error = $derived(store.errors[target] ?? null);
  let live = $derived(subject.sessions.some(isLive));
  let prompt = $derived(subject.excerpts?.prompt ?? null);
  let answerEnd = $derived(subject.excerpts?.answer_tail ?? subject.excerpts?.answer_head ?? null);

  function open(next: Mode): void {
    mode = mode === next ? "idle" : next;
    newName = subject.name;
    mergeTarget = null;
    routine = false;
  }

  async function done(accepted: Promise<boolean>): Promise<void> {
    if (await accepted) mode = "idle";
  }

  function rename(event: SubmitEvent): void {
    event.preventDefault();
    void done(store.rename(subject.id, newName));
  }
</script>

<article class="subject" data-testid="subject-card" data-subject={subject.id}>
  <header>
    <h3 class="name" data-testid="subject-name">{subject.name}</h3>
    <span class="project">{subject.project}</span>
    <span class="status {subject.status}" data-testid="subject-status">{SUBJECT_STATUS_TEXT[subject.status]}</span>
  </header>

  {#if subject.sessions.length > 0}
    <ul class="sessions">
      {#each subject.sessions as session (session.session_id)}
        <SessionRow {session} {store} />
      {/each}
    </ul>
  {:else}
    <p class="muted">No session yet.</p>
  {/if}

  {#if !live && (prompt || answerEnd)}
    <div class="excerpts" data-testid="subject-excerpts">
      {#if prompt}<p class="prompt" data-testid="subject-prompt">{prompt}</p>{/if}
      {#if answerEnd}<p class="answer" data-testid="subject-answer">{answerEnd}</p>{/if}
    </div>
  {/if}

  <div class="actions">
    <button class="tool" data-testid="subject-rename" aria-pressed={mode === "rename"} onclick={() => open("rename")}
      >Rename</button
    >
    {#if others.length > 0}
      <button class="tool" data-testid="subject-merge" aria-pressed={mode === "merge"} onclick={() => open("merge")}
        >Merge</button
      >
    {/if}
    <button class="tool" data-testid="subject-archive" aria-pressed={mode === "archive"} onclick={() => open("archive")}
      >Archive</button
    >
  </div>

  {#if mode === "rename"}
    <form class="panel" onsubmit={rename}>
      <input bind:value={newName} aria-label="New name for {subject.name}" data-testid="rename-input" />
      <button type="submit" class="tool" disabled={busy}>Save</button>
    </form>
  {:else if mode === "merge"}
    <div class="panel">
      <select bind:value={mergeTarget} aria-label="Merge {subject.name} into" data-testid="merge-target">
        <option value={null} disabled>Merge into...</option>
        {#each others as other (other.id)}
          <option value={other.id}>{other.name} ({other.project})</option>
        {/each}
      </select>
      <button
        class="tool"
        disabled={busy || mergeTarget === null}
        data-testid="merge-confirm"
        onclick={() => mergeTarget !== null && done(store.merge(subject.id, mergeTarget))}
      >
        Move its sessions there
      </button>
    </div>
  {:else if mode === "archive"}
    <div class="panel">
      <label class="routine">
        <input type="checkbox" bind:checked={routine} data-testid="routine-box" />
        Routine, could have run alone
      </label>
      <button
        class="tool"
        disabled={busy}
        data-testid="archive-confirm"
        onclick={() => done(store.archive(subject.id, routine))}
      >
        Archive {subject.name}
      </button>
    </div>
  {/if}

  {#if error}
    <p class="fault" role="alert" data-testid="action-error">{error}</p>
  {/if}
</article>

<style>
  .subject {
    padding: 12px 18px;
    border-radius: 16px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 6px 12px;
  }
  h3 {
    margin: 0;
    font-size: 16px;
    overflow-wrap: anywhere;
  }
  .project,
  .muted {
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .status {
    font-size: 12px;
    font-weight: 600;
    padding: 1px 9px;
    border-radius: var(--radius-pill);
    background: var(--color-border);
  }
  .sessions {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }
  .excerpts p {
    margin: 6px 0 0;
    font-size: 13.5px;
    color: var(--color-text-muted);
    overflow-wrap: anywhere;
  }
  .prompt::before {
    content: "> ";
    font-weight: 700;
  }
  .answer {
    font-style: italic;
  }
  .actions,
  .panel {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
  }
  .tool {
    min-height: 36px;
    padding: 0 14px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .tool[aria-pressed="true"] {
    background: var(--color-text);
    color: var(--color-bg);
  }
  input:not([type="checkbox"]),
  select {
    min-height: 36px;
    padding: 0 12px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-border);
    font: inherit;
  }
  .routine {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13.5px;
  }
  .fault {
    margin: 8px 0 0;
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
