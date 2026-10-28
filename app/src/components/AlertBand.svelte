<script lang="ts">
  // The alert band (FR-10, AD-175): under the header, above every other
  // banner, in every view and mode, while anyone is in alert. No dismiss: it
  // leaves when a board places every alerted session out of alert. The
  // theme never changes (FR-8).
  import type { Board } from "../lib/generated/Board";
  import { alertSummary, bandText } from "../lib/alert";
  import { slideMotion } from "../lib/motion";

  interface Props {
    board: Board | null;
    /** The store's clock tick, for "waiting N min". */
    nowMs: number;
  }
  let { board, nowMs }: Props = $props();

  let summary = $derived(board ? alertSummary(board) : null);
</script>

{#if summary}
  <div class="band" role="status" data-testid="alert-band" transition:slideMotion={{ y: -6 }}>
    <span class="pill">Alert</span>
    <span class="text" data-testid="alert-band-text">{bandText(summary, nowMs)}</span>
  </div>
{/if}

<style>
  .band {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 0 20px 12px;
    padding: 8px 16px;
    border-radius: var(--radius-surface);
    background: var(--color-attention);
    color: var(--color-attention-text);
    border-bottom: 3px solid var(--color-alert);
    font-weight: 600;
  }
  .pill {
    flex: none;
    padding: 2px 10px;
    border-radius: var(--radius-control);
    background: var(--color-alert);
    color: var(--color-on-alert);
    font-weight: 700;
    text-transform: uppercase;
    font-size: 12.5px;
    letter-spacing: 0.04em;
  }
  .text {
    min-width: 0;
  }
</style>
