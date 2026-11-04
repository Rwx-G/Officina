<script lang="ts">
  // The Orchestrator view (AD-93): the briefing Officina's own code builds,
  // the orchestrator's proposals, the composer over the approved templates
  // with the card core issued for its choices or for a proposal, and the
  // work order history. No model is involved. Leaving the view records the
  // briefing's reading (App.svelte).
  import type { Board } from "../lib/generated/Board";
  import type { Card } from "../lib/generated/Card";
  import { meeting } from "../lib/meeting.svelte";
  import { fadeMotion } from "../lib/motion";
  import type { OrchestratorStore } from "../lib/orchestrator.svelte";
  import Briefing from "./Briefing.svelte";
  import Composer from "./Composer.svelte";
  import OrchestratorStart from "./OrchestratorStart.svelte";
  import ProposalList from "./ProposalList.svelte";
  import WorkOrderCard from "./WorkOrderCard.svelte";
  import WorkOrderHistory from "./WorkOrderHistory.svelte";

  interface Props {
    store: OrchestratorStore;
    /** The board, for the names of the sessions answer proposals name. */
    board: Board | null;
    nowMs: number;
  }
  let { store, board, nowMs }: Props = $props();

  let view = $derived(store.view);
  let card = $derived(store.card);
  /** The history's row for the card, once its launch recorded one. */
  let order = $derived(card ? (view?.work_orders.find((o) => o.id === card.id) ?? null) : null);
  let subjectNames = $derived(new Map((view?.briefing.subjects.items ?? []).map((s) => [s.id, s.name])));
  let sessionNames = $derived(
    new Map(
      (board ? (Object.values(board.columns).flat() as Card[]) : [])
        .filter((c) => c.session_id !== null)
        .map((c) => [c.session_id!, c.name]),
    ),
  );
</script>

<main class="orchestrator-view" data-testid="orchestrator-view" in:fadeMotion|global>
  {#if !view}
    <p class="loading" data-testid="orchestrator-loading">Building the briefing...</p>
  {:else}
    <OrchestratorStart {store} {nowMs} />
    <Briefing briefing={view.briefing} {nowMs} />
    <ProposalList
      proposals={view.proposals}
      deleted={view.deleted_proposal_files}
      {subjectNames}
      {sessionNames}
      {nowMs}
      busy={store.composing || store.launching}
      error={store.proposalError}
      onOpen={(id) => void store.openProposal(id)}
      onDismiss={(id) => void store.dismissProposal(id)}
      onOpenMeeting={(session) => void meeting.receive(`sid:${session}`)}
    />
    <Composer {store} />
    {#if card}
      <WorkOrderCard
        {card}
        {order}
        launching={store.launching}
        launch={store.launch}
        {nowMs}
        busy={store.composing}
        shown={store.shown}
        onLaunch={() => void store.launchCard()}
        onPermission={(permission) => void store.setPermission(permission)}
        onBase={(base) => void store.chooseBase(base)}
      />
    {/if}
    <WorkOrderHistory orders={view.work_orders} {nowMs} />
  {/if}
  {#if store.readError}
    <p class="fault" role="alert" data-testid="briefing-read-error">
      The briefing's reading could not be recorded: {store.readError}
    </p>
  {/if}
</main>

<style>
  .orchestrator-view {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
    max-width: var(--reading-width);
    margin-inline: auto;
    padding: 0 0 20px;
  }
  .loading {
    padding: 40px 20px;
    color: var(--color-text-muted);
  }
  p {
    margin: 4px 0;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .fault {
    margin: 0 20px;
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
