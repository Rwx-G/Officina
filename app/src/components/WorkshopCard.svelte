<script lang="ts">
  // A workshop run on the board (AD-121, AD-122, FR-17): kind `workshop`,
  // placed by its state. Working shows the neutral "isolated" chip, Needs
  // you the "review" the run waits for, To resume a run Stop all kept, and
  // Done its badge, "no change" among them. Only a Needs you card wears
  // the attention colour (AD-55, D-163): a run working in its container
  // waits on nobody. Receive on a Needs you or Done card opens the review
  // panel. The run has no window to bring to front, and no session to
  // file or move.
  import type { Card } from "../lib/generated/Card";
  import { projectColor } from "../lib/colors";
  import { breathe } from "../lib/halo";
  import { ago } from "../lib/time";
  import { runIdOf, workshop } from "../lib/workshop.svelte";
  import { RUN_STATE_TEXT } from "../lib/workshop-text";

  interface Props {
    card: Card;
    nowMs: number;
    index?: number;
  }
  let { card, nowMs, index }: Props = $props();

  const BADGE_TEXT = {
    unknown: "unknown",
    interrupted: "interrupted",
    stopped: "stopped",
    ended: "ended",
    no_change: "no change",
  } as const;

  let runId = $derived(runIdOf(card.key));
  let since = $derived(card.last_event_at_ms ?? card.started_at_ms);
  let halo = $derived(card.column === "needs_you" ? { key: card.key, enteredAt: since } : null);
  let receivable = $derived(runId !== null && (card.column === "needs_you" || card.column === "done"));
</script>

<article
  class="card {card.column}"
  data-testid="card"
  data-kind="workshop"
  data-key={card.key}
  data-run-state={card.run_state}
  style:--project={projectColor(card.project.color)}
  use:breathe={halo}
>
  <div class="body">
    <div class="head">
      <span class="project"><span class="dot"></span>{card.project.name}</span>
      {#if index !== undefined && index < 9}<kbd aria-hidden="true">{index + 1}</kbd>{/if}
      {#if card.column === "working"}<span class="chip" data-testid="isolated-chip">isolated</span>{/if}
      {#if card.badge}<span class="badge" data-testid="badge">{BADGE_TEXT[card.badge]}</span>{/if}
    </div>
    <div class="name">{card.name}</div>
    {#if card.waiting_for}
      <div class="waiting" data-testid="waiting-for">{card.waiting_for}</div>
    {/if}
    <div class="meta">
      <span>workshop</span>
      {#if card.run_state}<span data-testid="run-state">{RUN_STATE_TEXT[card.run_state]}</span>{/if}
      {#if since !== null}<span>{ago(since, nowMs)}</span>{/if}
    </div>
  </div>
  {#if receivable && runId}
    <button class="link receive" data-testid="receive" onclick={() => void workshop.open(runId)}>Receive</button>
  {/if}
</article>

<style>
  .card {
    --edge: 1px;
    --stripe: 6px;
    position: relative;
    background: var(--color-surface);
    border: var(--edge) solid var(--color-border);
    border-left: var(--stripe) solid var(--project);
    border-radius: var(--radius-surface);
    padding: 4px;
  }
  .card.needs_you {
    border-color: var(--color-attention-border);
    border-left-color: var(--project);
  }
  .card.needs_you::after {
    content: "";
    position: absolute;
    inset: calc(-1 * var(--edge));
    left: calc(-1 * var(--stripe));
    border-radius: var(--radius-surface);
    box-shadow:
      0 0 0 3px var(--color-attention),
      0 0 0 7px color-mix(in srgb, var(--color-attention) 35%, transparent);
    opacity: var(--halo-rest);
    will-change: opacity;
    pointer-events: none;
    animation: halo-breathe var(--halo-half) ease-in-out var(--halo-delay) var(--halo-count) alternate;
  }
  .card.done,
  .card.to_resume {
    opacity: 0.86;
  }
  .body {
    padding: 12px 14px;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
  }
  .project {
    display: inline-flex;
    white-space: nowrap;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-muted);
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: var(--radius-round);
    background: var(--project);
  }
  kbd {
    margin-left: auto;
    font: inherit;
    font-size: 12px;
    padding: 0 7px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    color: var(--color-text-muted);
  }
  .chip,
  .badge {
    margin-left: auto;
    white-space: nowrap;
    font-size: 12px;
    font-weight: 600;
    padding: 1px 9px;
    border-radius: var(--radius-control);
    background: var(--color-border);
    color: var(--color-text);
  }
  kbd + .chip,
  kbd + .badge {
    margin-left: 0;
  }
  .name {
    margin-top: 4px;
    font-size: 16px;
    font-weight: 700;
    overflow-wrap: anywhere;
  }
  .waiting {
    display: inline-block;
    margin-top: 6px;
    padding: 2px 10px;
    border-radius: var(--radius-control);
    background: var(--color-attention);
    color: var(--color-attention-text);
    font-size: 13px;
    font-weight: 600;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    margin-top: 6px;
    font-size: 12.5px;
    color: var(--color-text-muted);
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
  }
  .receive {
    margin: 0 14px 8px;
  }
</style>
