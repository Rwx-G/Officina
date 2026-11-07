<script lang="ts">
  // The Workshop section of the Orchestrator view (AD-121, AD-122, FR-17):
  // where the workshop is set up and watched, fed by `get_workshop` and
  // `workshop://updated` through the workshop store. It holds no worker
  // text (D-1233). Nothing shows until the window read the view once.
  import type { WorkshopView } from "../lib/generated/WorkshopView";
  import { ENGINE_STATUS_TEXT } from "../lib/workshop-text";
  import BatteryStatus from "./BatteryStatus.svelte";
  import CredentialPanel from "./CredentialPanel.svelte";
  import ImageTable from "./ImageTable.svelte";
  import RunList from "./RunList.svelte";
  import TemplateTable from "./TemplateTable.svelte";

  interface Props {
    view: WorkshopView | null;
    nowMs: number;
  }
  let { view, nowMs }: Props = $props();
</script>

{#if view}
  <section class="block" data-testid="workshop-section" aria-labelledby="workshop-title">
    <h2 id="workshop-title">Workshop</h2>
    {#if !view.enabled}
      <p class="muted" data-testid="workshop-off">Off: the approved catalogue holds no valid [workshop] table.</p>
    {:else}
      <p data-testid="workshop-engine" data-status={view.engine?.status ?? "unread"}>
        {#if view.engine}
          {ENGINE_STATUS_TEXT[view.engine.status]}{#if view.engine.detail}<span class="muted">: {view.engine.detail}</span>{/if}
        {:else}
          <span class="muted">Docker not read yet.</span>
        {/if}
        {#if view.limits}
          <span class="muted" data-testid="workshop-limits-line"
            >; {view.limits.live} live of at most {view.limits.max_workers}, {view.limits.cpus} CPUs and {view.limits.memory_mib} MiB
            each</span
          >
        {/if}
      </p>
      {#if view.relay_stopped && !view.workshop_stopped}
        <p class="muted" data-testid="workshop-relay-stopped">The relay is stopped; the workshop is not.</p>
      {/if}
      <RunList {view} {nowMs} />
      <ImageTable images={view.images} {nowMs} />
      <BatteryStatus images={view.images} {nowMs} />
      <CredentialPanel credential={view.credential} network={view.network} />
      <TemplateTable templates={view.templates} share={view.share} failures={view.environment_failures} />
    {/if}
  </section>
{/if}

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
  p {
    margin: 4px 0;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .muted {
    color: var(--color-text-muted);
  }
</style>
