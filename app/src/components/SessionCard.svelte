<script lang="ts">
  import type { Card } from "../lib/generated/Card";
  import type { BoardStore } from "../lib/board.svelte";
  import { projectColor } from "../lib/colors";
  import { breathe } from "../lib/halo";
  import { outcomeText, visibleOutcome } from "../lib/outcome";
  import { ago, duration } from "../lib/time";
  import { drag, draggable } from "../lib/drag.svelte";
  import { targetsFor } from "../lib/settle";
  import { meeting, offersReceive } from "../lib/meeting.svelte";
  import CardMenu from "./CardMenu.svelte";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    card: Card;
    store: BoardStore;
    index?: number;
  }
  let { card, store, index }: Props = $props();

  const BADGE_TEXT = {
    unknown: "unknown",
    interrupted: "interrupted",
    stopped: "stopped",
    ended: "ended",
    no_change: "no change",
  } as const;
  /** Why a card is in Done though Claude Code did not say it finished. */
  const SETTLED_TEXT = { marked_done: "marked done", subject_archived: "subject archived" } as const;

  let shown = $derived(visibleOutcome(store.outcomes, card.key));
  let shownText = $derived(shown ? outcomeText(shown.outcome) : null);
  let since = $derived(card.last_event_at_ms ?? card.started_at_ms);
  let halo = $derived(card.column === "needs_you" ? { key: card.key, enteredAt: since } : null);
  let movable = $derived(targetsFor(card).length > 0);
  let dragged = $derived(drag.card?.key === card.key);
</script>

<article
  class="card {card.column}"
  class:movable
  class:dragged
  data-testid="card"
  data-key={card.key}
  style:--project={projectColor(card.project.color)}
  style:transform={dragged ? `translate(${drag.dx}px, ${drag.dy}px)` : undefined}
  use:breathe={halo}
  use:draggable={{ card, onDrop: (c, target) => void store.settle(c, target, false) }}
>
  <button
    class="main"
    onclick={() => store.focus(card)}
    title={card.kind === "background" ? "Open claude attach" : "Bring the session window to front"}
  >
    <div class="head">
      <span class="project"><span class="dot"></span>{card.project.name}</span>
      {#if index !== undefined && index < 9}<kbd aria-hidden="true">{index + 1}</kbd>{/if}
      {#if card.badge}<span class="badge" data-testid="badge">{BADGE_TEXT[card.badge]}</span>{/if}
      {#if card.settled}<span class="badge settled" data-testid="settled">{SETTLED_TEXT[card.settled]}</span>{/if}
    </div>
    <div class="name">{card.name}</div>
    {#if card.subject}
      <div class="subject" data-testid="card-subject" title="Subject">{card.subject.name}</div>
    {/if}
    {#if card.waiting_for}
      <div class="waiting" data-testid="waiting-for">{card.waiting_for}</div>
    {/if}
    {#if card.last_prompt}
      <p class="excerpt prompt" data-testid="last-prompt">{card.last_prompt}</p>
    {/if}
    {#if card.last_message}
      <p class="excerpt message" data-testid="last-message">{card.last_message}</p>
    {/if}
    <div class="meta">
      <span>{card.kind ?? "unknown kind"}</span>
      {#if since !== null}<span>{ago(since, store.nowMs)}</span>{/if}
      {#if card.started_at_ms !== null}<span>running {duration(store.nowMs - card.started_at_ms)}</span>{/if}
    </div>
  </button>

  {#if shownText}
    <div class="outcome" role="status" data-testid="focus-outcome">
      {shownText}
      {#if shown?.outcome.outcome === "candidates"}
        <ul>
          {#each shown.outcome.detail as title, i (i)}<li>{title}</li>{/each}
        </ul>
      {/if}
      {#if shown?.outcome.outcome === "focused"}
        <button class="link" onclick={() => store.wrongWindow(card)}>Wrong window?</button>
      {/if}
    </div>
  {/if}

  <CardMenu {card} busy={store.settling[card.key] === true} onChoose={(target) => void store.settle(card, target, true)} />

  {#if offersReceive(card)}
    <button class="link receive" data-testid="receive" onclick={() => void meeting.receive(card.key)}>Receive</button>
  {/if}

  {#if card.attach_available}
    <button class="link copy" data-testid="copy-attach" onclick={() => store.copyAttach(card)}>
      <StableLabel
        labels={{ copied: "Copied", ready: "Copy attach command" }}
        current={store.copied === card.key ? "copied" : "ready"}
      />
    </button>
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
  /* The ring around the card's border box, on its own layer: only its
     opacity animates (AD-65), and only once `breathe` has set its count. */
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
  .card.your_turn {
    --edge: 2px;
    border: var(--edge) solid var(--color-attention);
    border-left: var(--stripe) solid var(--project);
  }
  .card.done,
  .card.to_resume {
    opacity: 0.86;
  }
  /* An ended card that can go elsewhere: its menu sits at the top right,
     where the head keeps room for it. */
  .card.movable {
    cursor: grab;
  }
  .card.movable .head {
    padding-right: 30px;
  }
  /* Lifted while it is dragged. */
  .card.dragged {
    z-index: 10;
    opacity: 1;
    cursor: grabbing;
    box-shadow: 0 10px 24px var(--color-shadow);
  }
  .main {
    display: block;
    width: 100%;
    min-height: var(--target-min);
    padding: 12px 14px;
    border: 0;
    border-radius: calc(var(--radius-surface) - 4px);
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .main:hover {
    background: var(--color-bg);
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
  .badge {
    margin-left: auto;
    white-space: nowrap;
    font-size: 12px;
    font-weight: 600;
    padding: 1px 9px;
    border-radius: var(--radius-control);
    background: var(--color-border);
  }
  kbd + .badge,
  .badge + .badge {
    margin-left: 0;
  }
  .badge.settled {
    background: var(--color-bg);
    border: 1px solid var(--color-border);
  }
  .name {
    margin-top: 4px;
    font-size: 16px;
    font-weight: 700;
    overflow-wrap: anywhere;
  }
  .subject {
    margin-top: 2px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-muted);
    overflow-wrap: anywhere;
  }
  .subject::before {
    content: "# ";
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
  .excerpt {
    margin: 6px 0 0;
    font-size: 13.5px;
    color: var(--color-text-muted);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .prompt::before {
    content: "> ";
    font-weight: 700;
  }
  .message {
    font-style: italic;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin-top: 8px;
    font-size: 12.5px;
    color: var(--color-text-muted);
  }
  .outcome {
    margin: 0 14px 10px;
    padding: 8px 12px;
    border-radius: var(--radius-surface);
    background: var(--color-bg);
    font-size: 13px;
  }
  .outcome ul {
    margin: 4px 0 0;
    padding-left: 18px;
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
  .copy,
  .receive {
    margin: 0 14px 8px;
  }
</style>
