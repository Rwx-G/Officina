<script lang="ts">
  // Focus mode (FR-7, AD-66, D-114): only what waits for the author. The
  // approvals waiting, as one row that opens them on the board; the Needs
  // you sessions, numbered as the board numbers them, so the same digits
  // act; the Your turn sessions; the Working ones as a count. A row's
  // activation is the board card's, its outcome shown in the row. No scene,
  // no halo, no transition and no frame loop: nothing here moves.
  import { fadeMotion } from "../lib/motion";
  import type { BoardStore } from "../lib/board.svelte";
  import { causeText } from "../lib/alert";
  import { projectColor } from "../lib/colors";
  import type { Board } from "../lib/generated/Board";
  import type { Card } from "../lib/generated/Card";
  import { outcomeText, visibleOutcome } from "../lib/outcome";

  interface Props {
    board: Board;
    store: BoardStore;
    /** Opens the approvals where they are decided: Board mode. */
    onApprovals: () => void;
  }
  let { board, store, onApprovals }: Props = $props();

  let approvals = $derived(board.approvals.length);
  let needsYou = $derived(board.columns.needs_you);
  let yourTurn = $derived(board.columns.your_turn);
  let working = $derived(board.columns.working.length);
  let nothing = $derived(approvals === 0 && needsYou.length === 0 && yourTurn.length === 0);

  const plural = (n: number, one: string, many: string): string => `${n} ${n === 1 ? one : many}`;
</script>

{#snippet row(card: Card, index: number | null)}
  {@const shown = visibleOutcome(store.outcomes, card.key)}
  <li class="row {card.column}" data-testid="focus-row" data-key={card.key} style:--project={projectColor(card.project.color)}>
    <button
      class="main"
      onclick={() => store.focus(card)}
      title={card.kind === "background" ? "Open claude attach" : "Bring the session window to front"}
    >
      {#if index !== null && index < 9}<kbd aria-hidden="true">{index + 1}</kbd>{/if}
      <span class="project"><span class="dot"></span>{card.project.name}</span>
      <span class="name">{card.name}</span>
      {#if card.waiting_for}<span class="waiting" data-testid="focus-waiting-for">{card.waiting_for}</span>{/if}
      {#if card.scene?.alert}
        <span class="alert" data-testid="focus-alert">
          <span class="pill">Alert</span>
          {causeText(card, store.nowMs)}{#if card.scene.zone === "unknown"}, last known{/if}
        </span>
      {/if}
    </button>
    {#if shown}
      <div class="outcome" role="status" data-testid="focus-outcome">
        {outcomeText(shown.outcome)}
        {#if shown.outcome.outcome === "candidates"}
          <ul>
            {#each shown.outcome.detail as title, i (i)}<li>{title}</li>{/each}
          </ul>
        {/if}
      </div>
    {/if}
  </li>
{/snippet}

<main class="focus" data-testid="focus-list" in:fadeMotion|global>
  {#if approvals > 0}
    <button class="approvals" data-testid="focus-approvals" onclick={onApprovals} title="Open them on the board">
      {plural(approvals, "approval", "approvals")} waiting
    </button>
  {/if}
  {#if needsYou.length > 0}
    <h2>Needs you</h2>
    <ol class="rows" data-testid="focus-needs-you">
      {#each needsYou as card, index (card.key)}{@render row(card, index)}{/each}
    </ol>
  {/if}
  {#if yourTurn.length > 0}
    <h2>Your turn</h2>
    <ul class="rows" data-testid="focus-your-turn">
      {#each yourTurn as card (card.key)}{@render row(card, null)}{/each}
    </ul>
  {/if}
  {#if nothing}
    <p class="quiet" data-testid="focus-nothing">Nothing waits for you.</p>
  {/if}
  <p class="quiet" data-testid="focus-working">{plural(working, "session", "sessions")} working</p>
</main>

<style>
  .focus {
    max-width: 880px;
    margin: 0 auto;
    padding: 0 20px 20px;
  }
  h2 {
    margin: 18px 0 8px;
    font-size: 15px;
    color: var(--color-text-muted);
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .row {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-left: 6px solid var(--project);
    border-radius: var(--radius-surface);
  }
  .row.needs_you {
    border-color: var(--color-attention);
    border-left-color: var(--project);
  }
  .row.your_turn {
    outline: 2px solid var(--color-attention);
    outline-offset: -2px;
  }
  .main {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    width: 100%;
    min-height: var(--target-min);
    padding: 8px 14px;
    border: 0;
    border-radius: var(--radius-surface);
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .main:hover {
    background: var(--color-bg);
  }
  kbd {
    min-width: 22px;
    padding: 1px 6px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    font-weight: 700;
    text-align: center;
  }
  .project {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: var(--radius-round);
    background: var(--project);
  }
  .name {
    font-weight: 700;
  }
  .waiting {
    color: var(--color-text-muted);
  }
  .alert {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
  }
  .pill {
    padding: 2px 10px;
    border-radius: var(--radius-control);
    background: var(--color-alert);
    color: var(--color-on-alert);
    font-weight: 700;
    text-transform: uppercase;
    font-size: 12.5px;
    letter-spacing: 0.04em;
  }
  .outcome {
    padding: 0 14px 10px;
    font-size: 13px;
  }
  .outcome ul {
    margin: 4px 0 0;
    padding-left: 18px;
  }
  .approvals {
    display: block;
    width: 100%;
    min-height: var(--target-min);
    margin-top: 4px;
    padding: 8px 14px;
    border: 1px solid var(--color-attention-border);
    border-radius: var(--radius-surface);
    background: var(--color-attention);
    color: var(--color-attention-text);
    font-weight: 700;
    text-align: left;
    cursor: pointer;
  }
  .quiet {
    margin: 16px 0 0;
    color: var(--color-text-muted);
  }
</style>
