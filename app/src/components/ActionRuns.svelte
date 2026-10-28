<script lang="ts">
  // What one catalogue action starts, as text chips: the runner, the
  // program, then one chip per argument template, so a crafted template
  // cannot pass for two; for an HTTP action, its method, its URL template
  // and the credential it sends. Catalogue text comes from a floor a session
  // may have edited: it is rendered as text only.
  import type { CanonicalAction } from "../lib/generated/CanonicalAction";

  interface Props {
    action: CanonicalAction;
  }
  let { action }: Props = $props();
</script>

{#if action.http}
  <span class="runs" data-testid="action-http">
    <code class="chip" data-testid="http-method">{action.http.method}</code>
    <code class="chip" data-testid="http-url">{action.http.url}</code>
    {#if action.http.credential}
      with credential <code class="chip" data-testid="http-credential">{action.http.credential}</code>
    {/if}
  </span>
{:else}
  <span class="runs">
    {action.runner}{action.distro ? ` ${action.distro}` : ""}:
    <code class="chip">{action.program}</code>
    {#each action.args as arg, i (i)}<code class="chip">{arg}</code>{/each}
  </span>
{/if}

<style>
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .chip {
    display: inline-block;
    margin: 1px 4px 1px 0;
    padding: 1px 6px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    overflow-wrap: anywhere;
  }
</style>
