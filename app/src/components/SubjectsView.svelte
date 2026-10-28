<script lang="ts">
  // The Subjects view (FR-6, journey 1, AD-55): the registry's notes, the
  // open subjects with their sessions, the unfiled sessions by project and
  // the archived subjects on demand; the selected sessions are filed,
  // unfiled or start a subject from the bar above the lists. Story 3.7's
  // Triage panel heads the view, above the subjects. Every text renders as
  // text.
  import type { RegistryStore } from "../lib/registry.svelte";
  import { REGISTRY_STATE_TEXT } from "../lib/registry-text";
  import { clock } from "../lib/time";
  import ArchivedList from "./ArchivedList.svelte";
  import SubjectCard from "./SubjectCard.svelte";
  import SubjectPicker, { type PickerChoice } from "./SubjectPicker.svelte";
  import UnfiledSessions from "./UnfiledSessions.svelte";

  interface Props {
    store: RegistryStore;
    nowMs: number;
  }
  let { store, nowMs }: Props = $props();

  let view = $derived(store.view);
  let registry = $derived(view?.registry ?? null);
  let selected = $derived(store.selected.length);

  let newName = $state("");
  let newProject = $state<string | null>(null);
  let project = $derived(newProject ?? view?.projects[0] ?? null);

  function pick(choice: PickerChoice): void {
    if ("subject" in choice) void store.fileSelection(choice.subject);
    else void store.createFromSelection(choice.name);
  }

  async function create(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (project === null) return;
    if (await store.create(newName, project)) newName = "";
  }
</script>

<main class="subjects-view" data-testid="subjects-view">
  {#if !view}
    <p class="block muted" data-testid="subjects-loading">Reading the registry...</p>
  {:else}
    {#if registry && registry.state !== "ready"}
      <div class="banner" role="alert" data-testid="registry-unavailable">
        {REGISTRY_STATE_TEXT[registry.state] ?? "The registry does not exist yet: the window creates it when it starts."}
        {#if registry.detail}({registry.detail}){/if}
        Nothing can be filed or archived until it can be read.
      </div>
    {/if}
    {#if registry?.exclude_from_sync}
      <div class="note" role="note" data-testid="registry-synced">
        The registry lies in a folder a sync client manages. Exclude <code>{registry.exclude_from_sync}</code>
        from it: a client that writes an older copy back would undo what you decided.
      </div>
    {/if}
    {#if view.restore_notice}
      <div class="note" role="note" data-testid="registry-restored">
        The registry was found damaged and restored from the copy <code>{view.restore_notice.copy}</code>
        ({clock(view.restore_notice.restored_at_ms, nowMs)}): what you decided after that copy was taken is lost.
      </div>
    {/if}

    {#if selected > 0}
      <section class="block selection" data-testid="selection-bar" aria-label="Selected sessions">
        <p>
          {selected} {selected === 1 ? "session" : "sessions"} selected{store.selectionProject
            ? ` in ${store.selectionProject}`
            : ""}: file under a subject, or name a new one.
        </p>
        <SubjectPicker
          subjects={store.openSubjects}
          project={store.selectionProject}
          label="File the selected sessions under"
          disabled={store.inFlight["selection"] ?? false}
          onPick={pick}
        />
        <div class="row">
          <button class="tool" data-testid="unfile" onclick={() => store.fileSelection(null)}>Unfile</button>
          <button class="tool" onclick={() => (store.selected = [])}>Clear the selection</button>
        </div>
        {#if store.errors["selection"]}
          <p class="fault" role="alert" data-testid="action-error">{store.errors["selection"]}</p>
        {/if}
      </section>
    {/if}

    <section class="block" aria-labelledby="subjects-title">
      <h2 id="subjects-title">Subjects</h2>
      {#if view.subjects.length === 0}
        <p class="muted">No open subject.</p>
      {/if}
      <div class="list">
        {#each view.subjects as subject (subject.id)}
          <SubjectCard {subject} {store} others={view.subjects.filter((s) => s.id !== subject.id)} />
        {/each}
      </div>
      {#if view.projects.length > 0}
        <form class="row create" data-testid="new-subject" onsubmit={create}>
          <input bind:value={newName} placeholder="New subject" aria-label="New subject name" />
          <select
            value={project}
            onchange={(event) => (newProject = (event.currentTarget as HTMLSelectElement).value)}
            aria-label="Project of the new subject"
          >
            {#each view.projects as known (known)}
              <option value={known}>{known}</option>
            {/each}
          </select>
          <button type="submit" class="tool" disabled={newName.trim() === "" || (store.inFlight["create"] ?? false)}>
            Create subject
          </button>
        </form>
        {#if store.errors["create"]}
          <p class="fault" role="alert" data-testid="action-error">{store.errors["create"]}</p>
        {/if}
      {/if}
    </section>

    <UnfiledSessions unfiled={view.unfiled} {store} />
    <ArchivedList {store} />
  {/if}
</main>

<style>
  .subjects-view {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 0 20px 20px;
  }
  .block {
    padding: 12px 18px;
    border-radius: 16px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }
  .list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 12px;
  }
  h2 {
    margin: 0 0 8px;
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
  .banner {
    padding: 10px 16px;
    border-radius: 16px;
    background: var(--color-stopped-surface);
    border: 1px solid var(--color-stopped);
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .note {
    font-size: 13.5px;
    padding: 8px 16px;
    border-radius: 16px;
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    overflow-wrap: anywhere;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }
  .create {
    margin-top: 12px;
  }
  input,
  select {
    min-height: 36px;
    padding: 0 12px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-border);
    font: inherit;
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
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
