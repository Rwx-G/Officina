<script lang="ts">
  // The work order history (AD-92, AD-93, D-136): the latest orders with
  // their template, project, values, outcome and binding, the derived
  // "unbound" and "printed id differs" flags, and the bound session's state.
  // What `claude` printed and its error are untrusted, rendered as text.
  import type { WorkOrderView } from "../lib/generated/WorkOrderView";
  import { OUTCOME_TEXT, PHASE_TEXT, SOURCE_TEXT } from "../lib/orchestrator-text";
  import { clock } from "../lib/time";

  interface Props {
    orders: WorkOrderView[];
    nowMs: number;
  }
  let { orders, nowMs }: Props = $props();
</script>

<section class="block" data-testid="work-order-history" aria-labelledby="history-title">
  <h2 id="history-title">Work orders</h2>
  {#if orders.length === 0}
    <p class="muted" data-testid="history-empty">No work order launched yet.</p>
  {:else}
    <ul class="orders">
      {#each orders as order (order.id)}
        <li data-testid="work-order" data-outcome={order.outcome}>
          <div class="head">
            <strong data-testid="work-order-name">{order.name}</strong>
            <span>{order.template} in {order.project}</span>
            <span class="muted">{clock(order.launched_at_ms, nowMs)}, from the {SOURCE_TEXT[order.source]}</span>
          </div>
          {#if Object.keys(order.values).length > 0}
            <ul class="values" aria-label="Values">
              {#each Object.entries(order.values) as [name, value] (name)}
                <li class="chip"><span class="muted">{name}</span> {value}</li>
              {/each}
            </ul>
          {/if}
          <p data-testid="work-order-outcome">
            {OUTCOME_TEXT[order.outcome]}{order.exit_code !== null && order.outcome !== "started"
              ? `, exit ${order.exit_code}`
              : ""}
            {#if order.subject}
              <span class="muted">; subject {order.subject.name}</span>
            {:else if order.new_subject_name !== null}
              <span class="muted">; new subject {order.new_subject_name}</span>
            {/if}
          </p>
          <p data-testid="work-order-binding">
            {#if order.bg_id !== null}
              Bound to <code>{order.bg_id}</code>{order.bound_at_ms !== null ? ` at ${clock(order.bound_at_ms, nowMs)}` : ""}{order.session
                ? `: ${order.session.name ?? order.session.session_id}, ${PHASE_TEXT[order.session.phase]}`
                : ""}
            {:else if order.unbound}
              <span class="flag" data-testid="work-order-unbound">Unbound: no background session took its name within ten minutes.</span>
            {:else if order.outcome === "started"}
              <span class="muted">Waiting for its background session.</span>
            {:else}
              <span class="muted">No session.</span>
            {/if}
          </p>
          {#if order.printed_id_differs}
            <p class="flag" data-testid="work-order-printed-differs">
              What claude printed does not hold the id it bound to.
            </p>
          {/if}
          {#if order.error !== null}
            <pre class="text" data-testid="work-order-error">{order.error}</pre>
          {/if}
          {#if order.printed !== null}
            <details>
              <summary>What claude printed</summary>
              <pre class="text" data-testid="work-order-printed">{order.printed}</pre>
            </details>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

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
  .orders {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .orders > li {
    padding: 8px 0;
    border-top: 1px solid var(--color-border);
    font-size: 13.5px;
  }
  .orders > li:first-child {
    border-top: none;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    align-items: baseline;
  }
  .values {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 4px 0;
    padding: 0;
    list-style: none;
  }
  .chip {
    padding: 1px 8px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .flag {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
  .muted {
    color: var(--color-text-muted);
  }
  code,
  .text {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .text {
    margin: 4px 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
