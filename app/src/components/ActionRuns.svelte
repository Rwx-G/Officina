<script lang="ts">
  // What one catalogue action starts, as text chips: the runner, the
  // program, then one chip per argument template, so a crafted template
  // cannot pass for two. Catalogue text comes from a floor a session may
  // have edited: it is rendered as text only.
  import type { CanonicalAction } from "../lib/generated/CanonicalAction";

  interface Props {
    action: CanonicalAction;
  }
  let { action }: Props = $props();
</script>

<span class="runs">
  {action.runner}{action.distro ? ` ${action.distro}` : ""}:
  <code class="chip">{action.program}</code>
  {#each action.args as arg, i (i)}<code class="chip">{arg}</code>{/each}
</span>

<style>
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .chip {
    display: inline-block;
    margin: 1px 4px 1px 0;
    padding: 1px 6px;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    overflow-wrap: anywhere;
  }
</style>
