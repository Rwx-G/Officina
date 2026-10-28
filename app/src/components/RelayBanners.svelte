<script lang="ts">
  // The Board view's relay banners (AD-35, Slice A): a stopped relay, a relay
  // floor that cannot be read, and a pending catalogue change with its diff
  // and the Approve button. Catalogue text comes from a floor a session may
  // have edited: it is rendered as text only.
  import type { RelayStore } from "../lib/relay.svelte";
  import { clock } from "../lib/time";
  import PendingCatalogue from "./PendingCatalogue.svelte";

  interface Props {
    store: RelayStore;
    nowMs: number;
  }
  let { store, nowMs }: Props = $props();

  let relay = $derived(store.relay);
</script>

{#if relay?.stopped}
  <div class="banner stopped" role="alert" data-testid="relay-stopped-banner">
    The relay is stopped{relay.stopped_since_ms !== null ? ` since ${clock(relay.stopped_since_ms, nowMs)}` : ""}:
    <code>officina run</code> refuses every action until you resume it. The gate still guards your projects.
  </div>
{/if}

{#if store.inUse && relay?.floor_error}
  <div class="banner stopped" role="alert" data-testid="relay-floor-error">
    Relay floor cannot be used: {relay.floor_error}. <code>officina run</code> refuses every action until it can.
  </div>
{/if}

<PendingCatalogue {store} />

<style>
  .banner {
    margin: 0 20px 12px;
    padding: 10px 16px;
    border-radius: var(--radius-surface);
    color: var(--color-text);
  }
  .stopped {
    background: var(--color-stopped-surface);
    border: 1px solid var(--color-stopped);
    font-weight: 600;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
</style>
