<script lang="ts">
  // The sessions under no subject, by project (FR-6, D-96): each can be
  // selected to file or to start a subject; non-interactive runs are
  // counted, the most recent listed on demand.
  import type { UnfiledProject } from "../lib/generated/UnfiledProject";
  import type { RegistryStore } from "../lib/registry.svelte";
  import SessionRow from "./SessionRow.svelte";

  interface Props {
    unfiled: UnfiledProject[];
    store: RegistryStore;
  }
  let { unfiled, store }: Props = $props();

  let expanded = $state<Record<string, boolean>>({});
</script>

<section class="block" data-testid="unfiled" aria-labelledby="unfiled-title">
  <h2 id="unfiled-title">Unfiled sessions</h2>
  {#if unfiled.length === 0}
    <p class="muted">Every session is filed.</p>
  {/if}
  {#each unfiled as group (group.project)}
    <div class="group" data-testid="unfiled-project" data-project={group.project}>
      <h3>{group.project}</h3>
      {#if group.sessions.length > 0}
        <ul>
          {#each group.sessions as session (session.session_id)}
            <SessionRow {session} {store} />
          {/each}
        </ul>
      {/if}
      {#if group.non_interactive_count > 0}
        <button
          class="link"
          aria-expanded={expanded[group.project] ?? false}
          data-testid="non-interactive-toggle"
          onclick={() => (expanded = { ...expanded, [group.project]: !(expanded[group.project] ?? false) })}
        >
          {group.non_interactive_count} non-interactive {group.non_interactive_count === 1 ? "run" : "runs"}
        </button>
        {#if expanded[group.project]}
          <ul data-testid="non-interactive">
            {#each group.non_interactive as session (session.session_id)}
              <SessionRow {session} {store} />
            {/each}
          </ul>
          {#if group.non_interactive_count > group.non_interactive.length}
            <p class="muted">The {group.non_interactive.length} most recent are listed.</p>
          {/if}
        {/if}
      {/if}
    </div>
  {/each}
</section>

<style>
  .block {
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
    margin: 10px 0 2px;
    font-size: 14px;
    overflow-wrap: anywhere;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .muted {
    margin: 4px 0;
    font-size: 13px;
    color: var(--color-text-muted);
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
</style>
