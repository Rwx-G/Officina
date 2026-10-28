<script lang="ts">
  // The archived subjects, on demand (FR-6, AD-54): each with its routine
  // mark and a Reopen, which makes it `to_resume` (AD-48). Merged subjects
  // are not listed (D-55).
  import { subjectTarget, type RegistryStore } from "../lib/registry.svelte";

  interface Props {
    store: RegistryStore;
  }
  let { store }: Props = $props();

  let archived = $derived(store.archived);
  let listError = $derived(store.errors["archived"] ?? null);
</script>

<section class="block" data-testid="archived" aria-labelledby="archived-title">
  <h2 id="archived-title">Archived subjects</h2>
  {#if archived === null}
    <button
      class="tool"
      data-testid="show-archived"
      disabled={store.inFlight["archived"] ?? false}
      onclick={() => store.loadArchived()}
    >
      Show archived subjects
    </button>
  {:else}
    {#if archived.length === 0}
      <p class="muted">No archived subject.</p>
    {/if}
    <ul>
      {#each archived as subject (subject.id)}
        {@const error = store.errors[subjectTarget(subject.id)]}
        <li data-testid="archived-subject" data-subject={subject.id}>
          <span class="name">{subject.name}</span>
          <span class="muted">{subject.project}</span>
          {#if subject.routine}<span class="tag" data-testid="routine">routine</span>{/if}
          <button
            class="tool"
            disabled={store.inFlight[subjectTarget(subject.id)] ?? false}
            onclick={() => store.reopen(subject.id)}
          >
            Reopen
          </button>
          {#if error}<p class="fault" role="alert" data-testid="action-error">{error}</p>{/if}
        </li>
      {/each}
    </ul>
    <button class="tool" onclick={() => (store.archived = null)}>Hide archived subjects</button>
  {/if}
  {#if listError}<p class="fault" role="alert">{listError}</p>{/if}
</section>

<style>
  .block {
    padding: 12px 18px;
    border-radius: 16px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }
  h2 {
    margin: 0 0 6px;
    font-size: 16px;
  }
  ul {
    margin: 0 0 8px;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    min-height: 44px;
  }
  .name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .muted {
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .tag {
    font-size: 12px;
    padding: 0 8px;
    border-radius: var(--radius-pill);
    background: var(--color-border);
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
    flex-basis: 100%;
    margin: 0;
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
