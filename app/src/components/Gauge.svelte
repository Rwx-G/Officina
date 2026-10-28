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
    /** Two lines, for the one-line header of Scene mode (D-525): the track
     *  beside the value, the freshness after the explanation. */
    compact?: boolean;
  }
  let { label, window, weekly, writtenAtMs, staleAfterMs, nowMs, compact = false }: Props = $props();

  let used = $derived(window ? Math.round(window.used_percentage) : null);
  let resetPassed = $derived(window ? window.resets_at * 1000 <= nowMs : false);
  let stale = $derived(writtenAtMs === null || nowMs - writtenAtMs > staleAfterMs);
  let explanation = $derived.by(() => {
    if (!window) return "no data: see officina doctor";
    if (resetPassed) return "reset, no new data";
    const reset = `resets at ${clock(window.resets_at * 1000, nowMs)}`;
    return weekly ? `${reset}, ${weekElapsedPercent(window.resets_at, nowMs)} % of the week elapsed` : reset;
  });
  let freshness = $derived(
    window && writtenAtMs !== null ? `${stale ? "stale, " : ""}updated ${ago(writtenAtMs, nowMs)}` : null,
  );
  // Stale first: on one cut line, the freshness that matters stays seen.
  let line = $derived(freshness === null ? explanation : stale ? `${freshness}, ${explanation}` : `${explanation}, ${freshness}`);
</script>

<!-- A fixed width (D-185): the 30 s tick rewords the text, never the
     gauge's size, so neither the header nor the scene's scale follows it.
     A line longer than the gauge is cut, its whole text in the tooltip. -->
<div
  class="gauge"
  class:compact
  class:muted={!window || resetPassed || stale}
  data-testid="gauge-{weekly ? 'week' : '5h'}"
>
  {#if compact}
    <div class="top">
      <span class="label">{label}</span>
      <div class="track" role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={used ?? 0}>
        <div class="fill" style:width="{resetPassed ? 0 : (used ?? 0)}%"></div>
      </div>
      <span class="value">{used === null || resetPassed ? "--" : `${used} %`}</span>
    </div>
    <div class="explain" class:stale title={line}>{line}</div>
  {:else}
    <div class="top">
      <span class="label">{label}</span>
      <span class="value">{used === null || resetPassed ? "--" : `${used} %`}</span>
    </div>
    <div class="track" role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={used ?? 0}>
      <div class="fill" style:width="{resetPassed ? 0 : (used ?? 0)}%"></div>
    </div>
    <div class="explain" title={explanation}>{explanation}</div>
    {#if freshness !== null}
      <div class="fresh" class:stale>{freshness}</div>
    {/if}
  {/if}
</div>

<style>
  .gauge {
    width: var(--gauge-width);
    padding: 8px 14px;
    border-radius: var(--radius-surface);
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
    border-radius: var(--radius-control);
    background: var(--color-border);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--color-working);
    border-radius: var(--radius-control);
  }
  .explain,
  .fresh {
    font-size: 12.5px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .stale {
    font-weight: 600;
  }
  .muted .fill {
    background: var(--color-text-muted);
  }
  .compact {
    width: var(--gauge-width-compact);
    padding: 3px 12px;
  }
  .compact .top {
    align-items: center;
    gap: 10px;
    font-size: 13.5px;
    line-height: 1.3;
  }
  .compact .track {
    flex: 1 1 0;
    height: 6px;
    margin: 0;
  }
  .compact .explain {
    font-size: 12px;
    line-height: 1.3;
  }
</style>
