<script lang="ts">
  import type { Card } from "../lib/generated/Card";
  import type { Column } from "../lib/generated/Column";
  import type { ApprovalView } from "../lib/generated/ApprovalView";
  import type { BoardStore } from "../lib/board.svelte";
  import ApprovalCard from "./ApprovalCard.svelte";
  import SessionCard from "./SessionCard.svelte";

  interface Props {
    column: Column;
    title: string;
    hint: string;
    cards: Card[];
    store: BoardStore;
    collapseAfter?: number;
    numbered?: boolean;
    /** Approval requests, listed before the session cards (AD-35). */
    approvals?: ApprovalView[];
  }
  let { column, title, hint, cards, store, collapseAfter, numbered = false, approvals = [] }: Props = $props();

  let expanded = $state(false);
  let visible = $derived(
    collapseAfter !== undefined && !expanded ? cards.slice(0, collapseAfter) : cards,
  );
  let hidden = $derived(cards.length - visible.length);
</script>

<section class="column {column}" data-testid="column-{column}" aria-labelledby="title-{column}">
  <header>
    <h2 id="title-{column}" tabindex="-1">{title}</h2>
    <span class="count" data-testid="count-{column}">{cards.length + approvals.length}</span>
  </header>
  <p class="hint">{hint}</p>
  <div class="cards">
    {#each approvals as approval (approval.id)}
      <ApprovalCard
        {approval}
        busy={store.deciding[approval.id] === true}
        error={store.approvalErrors[approval.id]}
        nowMs={store.nowMs}
        onDecide={(approve) => void store.decideApproval(approval.id, approve)}
      />
    {/each}
    {#each visible as card, i (card.key)}
      <SessionCard {card} {store} index={numbered ? i : undefined} />
    {:else}
      {#if approvals.length === 0}<p class="empty">Nothing here.</p>{/if}
    {/each}
  </div>
  {#if hidden > 0}
    <button class="more" onclick={() => (expanded = true)}>Show {hidden} more</button>
  {:else if expanded && collapseAfter !== undefined && cards.length > collapseAfter}
    <button class="more" onclick={() => (expanded = false)}>Show less</button>
  {/if}
</section>

<style>
  .column {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    padding: 14px;
    border-radius: var(--radius-surface);
    background: var(--color-column);
    border: 1px solid var(--color-border);
  }
  .column.needs_you {
    background: var(--color-attention-wash);
    border: 2px solid var(--color-attention-border);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h2 {
    margin: 0;
    padding: 4px 14px;
    font-size: 14px;
    font-weight: 700;
    border-radius: var(--radius-control);
    background: var(--color-text);
    color: var(--color-bg);
  }
  .needs_you h2 {
    background: var(--color-attention);
    color: var(--color-attention-text);
  }
  .count {
    font-weight: 700;
    color: var(--color-text-muted);
  }
  .hint {
    margin: 0;
    font-size: 12.5px;
    color: var(--color-text-muted);
  }
  .cards {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .empty {
    margin: 4px 0;
    font-size: 13px;
    color: var(--color-text-muted);
  }
  .more {
    min-height: var(--target-min);
    border: 1px dashed var(--color-border);
    border-radius: var(--radius-control);
    background: transparent;
    cursor: pointer;
    font-weight: 600;
  }
</style>
