<script lang="ts">
  import type { QuotaWindow } from "../lib/generated/QuotaWindow";
  import { ago, clock, weekElapsedPercent } from "../lib/time";

  interface Props {
    label: string;
    window: QuotaWindow | null;
    weekly: boolean;
    writtenAtMs: number | null;
    staleAfterMs: number;
    nowMs: number;
  }
  let { label, window, weekly, writtenAtMs, staleAfterMs, nowMs }: Props = $props();

  let used = $derived(window ? Math.round(window.used_percentage) : null);
  let resetPassed = $derived(window ? window.resets_at * 1000 <= nowMs : false);
  let stale = $derived(writtenAtMs === null || nowMs - writtenAtMs > staleAfterMs);
  let explanation = $derived.by(() => {
    if (!window) return "no data: see officina doctor";
    if (resetPassed) return "reset, no new data";
    const reset = `resets at ${clock(window.resets_at * 1000, nowMs)}`;
    return weekly ? `${reset}, ${weekElapsedPercent(window.resets_at, nowMs)} % of the week elapsed` : reset;
  });
</script>

<div class="gauge" class:muted={!window || resetPassed || stale} data-testid="gauge-{weekly ? 'week' : '5h'}">
  <div class="top">
    <span class="label">{label}</span>
    <span class="value">{used === null || resetPassed ? "--" : `${used} %`}</span>
  </div>
  <div class="track" role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={used ?? 0}>
    <div class="fill" style:width="{resetPassed ? 0 : (used ?? 0)}%"></div>
  </div>
  <div class="explain">{explanation}</div>
  {#if window && writtenAtMs !== null}
    <div class="fresh" class:stale>{stale ? "stale, " : ""}updated {ago(writtenAtMs, nowMs)}</div>
  {/if}
</div>

<style>
  .gauge {
    min-width: 200px;
    padding: 8px 14px;
    border-radius: 16px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }
  .top {
    display: flex;
    justify-content: space-between;
    font-weight: 600;
  }
  .value {
    font-weight: 700;
  }
  .track {
    height: 8px;
    margin: 6px 0 4px;
    border-radius: var(--radius-pill);
    background: var(--color-border);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--color-working);
    border-radius: var(--radius-pill);
  }
  .explain,
  .fresh {
    font-size: 12.5px;
    color: var(--color-text-muted);
  }
  .stale {
    font-weight: 600;
  }
  .muted .fill {
    background: var(--color-text-muted);
  }
</style>
