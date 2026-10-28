<script lang="ts">
  // One session in the Subjects view: a box to select it for a move, its
  // name and its state, live from its board card or how it ended. Session
  // text renders as text.
  import type { SessionView } from "../lib/generated/SessionView";
  import { isLive, type RegistryStore } from "../lib/registry.svelte";
  import { END_TEXT, LIVE_STATE_TEXT } from "../lib/registry-text";

  interface Props {
    session: SessionView;
    store: RegistryStore;
  }
  let { session, store }: Props = $props();

  let label = $derived(session.name ?? `session ${session.session_id.slice(-8)}`);
  let state = $derived.by(() => {
    const card = session.card;
    if (card && isLive(session)) {
      return card.waiting_for ? `${LIVE_STATE_TEXT[card.column]}: ${card.waiting_for}` : LIVE_STATE_TEXT[card.column];
    }
    if (session.ended) {
      const how = END_TEXT[session.ended.how];
      return session.ended.reason ? `${how} (${session.ended.reason})` : how;
    }
    return "not live";
  });
</script>

<li class="session" class:live={isLive(session)} data-testid="session-row" data-session={session.session_id}>
  <label>
    <input
      type="checkbox"
      checked={store.selected.includes(session.session_id)}
      onchange={() => store.toggle(session.session_id)}
      aria-label="Select {label}"
    />
    <span class="name">{label}</span>
  </label>
  <span class="state" data-testid="session-state">{state}</span>
  {#if session.superseded}<span class="tag">replaced</span>{/if}
</li>

<style>
  .session {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    min-height: 36px;
    font-size: 13.5px;
  }
  label {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }
  input {
    width: 18px;
    height: 18px;
  }
  .name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .state {
    color: var(--color-text-muted);
  }
  .live .state {
    color: var(--color-working);
    font-weight: 600;
  }
  .tag {
    font-size: 12px;
    padding: 0 8px;
    border-radius: var(--radius-control);
    background: var(--color-border);
  }
</style>
