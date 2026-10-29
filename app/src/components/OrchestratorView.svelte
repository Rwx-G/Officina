<script lang="ts">
  // The Orchestrator view (AD-93): the briefing Officina's own code builds,
  // the templates the composer will offer (story 5.9 puts the composer in
  // their slot), and the work order history. No model is involved. Leaving
  // the view records the briefing's reading (App.svelte).
  import { fadeMotion } from "../lib/motion";
  import type { OrchestratorStore } from "../lib/orchestrator.svelte";
  import { TEMPLATE_CHANGE_TEXT } from "../lib/orchestrator-text";
  import Briefing from "./Briefing.svelte";
  import WorkOrderHistory from "./WorkOrderHistory.svelte";

  interface Props {
    store: OrchestratorStore;
    nowMs: number;
  }
  let { store, nowMs }: Props = $props();

  let view = $derived(store.view);
</script>

<main class="orchestrator-view" data-testid="orchestrator-view" in:fadeMotion|global>
  {#if !view}
    <p class="loading" data-testid="orchestrator-loading">Building the briefing...</p>
  {:else}
    <Briefing briefing={view.briefing} {nowMs} />

    <section class="block" data-testid="composer-slot" aria-labelledby="templates-title">
      <h2 id="templates-title">Templates</h2>
      {#if view.templates.length === 0}
        <p class="muted" data-testid="templates-none">
          No template is approved: add a <code>[[template]]</code> to the relay floor and approve the catalogue in the Relay
          view.
        </p>
      {:else}
        <ul>
          {#each view.templates as template (template.name)}
            <li data-testid="template-option">
              <strong>{template.name}</strong>: {template.description}
              <span class="muted">({template.projects.join(", ")})</span>
            </li>
          {/each}
        </ul>
      {/if}
      {#if view.awaiting_templates.length > 0}
        <h3>Awaiting your approval</h3>
        <ul>
          {#each view.awaiting_templates as awaiting (awaiting.name)}
            <li data-testid="template-awaiting" data-change={awaiting.change}>
              <strong>{awaiting.name}</strong>
              {TEMPLATE_CHANGE_TEXT[awaiting.change]}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <WorkOrderHistory orders={view.work_orders} {nowMs} />
  {/if}
  {#if store.readError}
    <p class="fault" role="alert" data-testid="briefing-read-error">
      The briefing's reading could not be recorded: {store.readError}
    </p>
  {/if}
</main>

<style>
  .orchestrator-view {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
    max-width: var(--reading-width);
    margin-inline: auto;
    padding: 0 0 20px;
  }
  .loading {
    padding: 40px 20px;
    color: var(--color-text-muted);
  }
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
    margin: 10px 0 4px;
    font-size: 14px;
  }
  p,
  ul {
    margin: 4px 0;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  ul {
    padding-left: 18px;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .fault {
    margin: 0 20px;
    color: var(--color-stopped-text);
    font-weight: 600;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
</style>
