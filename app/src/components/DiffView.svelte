<script lang="ts">
  // The meeting's diff (AD-84, AD-93): its files, each with a jump to its
  // section and to each of its hunks, then the diff itself as monospace
  // text whose `+` and `-` prefixes mark the lines; no colour (D-564).
  // Paths and lines came from a session's repository: rendered as text.
  import type { DiffView } from "../lib/generated/DiffView";

  interface Props {
    diff: DiffView;
  }
  let { diff }: Props = $props();

  let lines = $derived(diff.text === "" ? [] : diff.text.replace(/\n$/, "").split("\n"));
  let body = $state<HTMLElement | null>(null);

  /** Brings line `at` (from 0) to the top of the diff and focuses it. */
  function jump(at: number): void {
    const line = body?.querySelector<HTMLElement>(`[data-line="${at}"]`);
    line?.scrollIntoView({ block: "start" });
    line?.focus({ preventScroll: true });
  }
</script>

<div class="diff-view" data-testid="diff-view">
  {#if diff.files.length > 0}
    <ul class="files" data-testid="diff-files">
      {#each diff.files as file, i (i)}
        <li>
          <button type="button" class="link path" data-testid="diff-file" onclick={() => jump(file.line)}>{file.path}</button>
          {#each file.hunks as hunk, j (hunk)}
            <button type="button" class="link hunk" data-testid="diff-hunk" aria-label="Hunk {j + 1} of {file.path}" onclick={() => jump(hunk)}>
              hunk {j + 1}
            </button>
          {/each}
        </li>
      {/each}
    </ul>
  {:else}
    <p class="muted" data-testid="diff-empty">No difference since the session started.</p>
  {/if}
  {#if diff.truncated}
    <p class="muted" data-testid="diff-truncated">The diff is longer than the meeting shows; its end is cut.</p>
  {/if}
  {#if lines.length > 0}
    <pre class="body" data-testid="diff-text" bind:this={body}>{#each lines as line, i (i)}<span class="line" data-line={i} tabindex="-1">{line}</span>{/each}</pre>
  {/if}
</div>

<style>
  .files {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .files li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px 10px;
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
    overflow-wrap: anywhere;
    text-align: left;
  }
  .path {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 13px;
  }
  .hunk {
    font-size: 12.5px;
  }
  .body {
    margin: 0;
    max-height: 420px;
    overflow: auto;
    padding: 8px 10px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    background: var(--color-surface);
    color: var(--color-text);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
    line-height: 1.45;
  }
  .line {
    display: block;
    min-height: 1.45em;
    white-space: pre;
    outline-offset: -2px;
  }
  .muted {
    margin: 0 0 6px;
    color: var(--color-text-muted);
    font-size: 13px;
  }
</style>
