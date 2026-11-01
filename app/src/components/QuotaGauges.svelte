<script lang="ts">
  // The quota gauges, data-driven (story 4.20, D-1042): a window shows when
  // the quota data carries it. Claude Code sends each window of
  // `rate_limits` only while the account has it, so a five-hour window the
  // data leaves out takes no room. The weekly gauge always shows: it is
  // the place that says "no data" while no quota file exists, which keeps
  // the header's anchor where it was. It comes first, so the five-hour
  // gauge coming or going after it moves no other gauge (D-517).
  import type { QuotaView } from "../lib/generated/QuotaView";
  import Gauge from "./Gauge.svelte";

  interface Props {
    quota: QuotaView | null;
    nowMs: number;
  }
  let { quota, nowMs }: Props = $props();
</script>

<Gauge
  label="Week"
  window={quota?.seven_day ?? null}
  weekly={true}
  writtenAtMs={quota?.written_at_ms ?? null}
  staleAfterMs={quota?.stale_after_ms ?? 0}
  {nowMs}
/>
{#if quota?.five_hour}
  <Gauge
    label="5 h"
    window={quota.five_hour}
    weekly={false}
    writtenAtMs={quota.written_at_ms}
    staleAfterMs={quota.stale_after_ms}
    {nowMs}
  />
{/if}
