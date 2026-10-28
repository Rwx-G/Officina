<script lang="ts">
  import type { Card } from "../lib/generated/Card";
  import type { BoardStore } from "../lib/board.svelte";
  import { projectColor } from "../lib/colors";
  import { ago, duration } from "../lib/time";

  interface Props {
    card: Card;
    store: BoardStore;
    index?: number;
  }
  let { card, store, index }: Props = $props();

  const BADGE_TEXT = { unknown: "unknown", interrupted: "interrupted", stopped: "stopped", ended: "ended" } as const;

  let shown = $derived(store.outcomes[card.key]?.pending ? undefined : store.outcomes[card.key]);
  let outcomeText = $derived.by(() => {
    if (!shown) return null;
    const o = shown.outcome;
    switch (o.outcome) {
      case "focused":
        return "Window brought to front";
      case "attach_opened":
        return "Attach console opened";
      case "candidates":
        return "Several windows could match, none chosen:";
      case "no_window":
        return "No window found for this session";
      case "refused":
        return "Windows refused the switch; the window is flashing";
      case "unsupported":
        return "Not supported on this platform";
      case "failed":
        return `Could not open: ${o.detail}`;
    }
  });
  let since = $derived(card.last_event_at_ms ?? card.started_at_ms);
</script>

<article
  class="card {card.column}"
  data-testid="card"
  data-key={card.key}
  style:--project={projectColor(card.project.color)}
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

  {#if outcomeText}
    <div class="outcome" role="status" data-testid="focus-outcome">
      {outcomeText}
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

  {#if card.attach_available}
    <button class="link copy" onclick={() => store.copyAttach(card)}>
      {store.copied === card.key ? "Copied" : "Copy attach command"}
    </button>
  {/if}
</article>

<style>
  .card {
    position: relative;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-left: 6px solid var(--project);
    border-radius: var(--radius-card);
    padding: 4px;
  }
  .card.needs_you {
    border-color: var(--color-attention-border);
    border-left-color: var(--project);
    animation: halo-breath 2.4s ease-in-out infinite;
  }
  .card.your_turn {
    border: 2px solid var(--color-attention);
    border-left: 6px solid var(--project);
  }
  .card.done,
  .card.to_resume {
    opacity: 0.86;
  }
  .main {
    display: block;
    width: 100%;
    min-height: var(--target-min);
    padding: 12px 14px;
    border: 0;
    border-radius: calc(var(--radius-card) - 4px);
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .main:hover {
    background: var(--color-bg);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .project {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-muted);
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--project);
  }
  kbd {
    margin-left: auto;
    font: inherit;
    font-size: 12px;
    padding: 0 7px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-border);
    color: var(--color-text-muted);
  }
  .badge {
    margin-left: auto;
    font-size: 12px;
    font-weight: 600;
    padding: 1px 9px;
    border-radius: var(--radius-pill);
    background: var(--color-border);
  }
  kbd + .badge {
    margin-left: 0;
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
    border-radius: var(--radius-pill);
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
    border-radius: 14px;
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
  .copy {
    margin: 0 14px 8px;
  }
</style>
