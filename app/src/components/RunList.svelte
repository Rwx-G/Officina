<script lang="ts">
  // The workshop's runs (AD-116, AD-121): the live ones with Stop all, the
  // ones Stop all kept with Resume, and the orphan containers of this
  // floor. Stop all and remove only reduce, so the keyboard reaches them;
  // Resume opens its dialog, whose Resume is pointer only (D-28).
  import type { RemoveRunAnswer } from "../lib/generated/RemoveRunAnswer";
  import type { WorkshopView } from "../lib/generated/WorkshopView";
  import { ago } from "../lib/time";
  import { workshop } from "../lib/workshop.svelte";
  import { REMOVE_REFUSAL_TEXT, RUN_STATE_TEXT, STOP_ALL_TEXT } from "../lib/workshop-text";
  import ResumeDialog from "./ResumeDialog.svelte";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    view: WorkshopView;
    nowMs: number;
  }
  let { view, nowMs }: Props = $props();

  let resumeOpen = $state(false);

  /** Orphan containers grouped by the run their label names: one removal
   *  takes every object of the run. */
  let orphans = $derived.by(() => {
    const groups: { runId: string | null; names: string[] }[] = [];
    for (const orphan of view.runs.orphans) {
      const group = groups.find((g) => g.runId === orphan.run_id);
      if (group) group.names.push(orphan.name);
      else groups.push({ runId: orphan.run_id, names: [orphan.name] });
    }
    return groups;
  });

  function removedText(answer: RemoveRunAnswer | string): string {
    if (typeof answer === "string") return answer;
    return answer.status === "removed"
      ? `Removed ${answer.detail} object${answer.detail === 1 ? "" : "s"}.`
      : REMOVE_REFUSAL_TEXT[answer.detail];
  }

  function seconds(ms: number): string {
    return (ms / 1000).toFixed(1);
  }
</script>

<div class="part" data-testid="run-list">
  <div class="head">
    <h3>Runs</h3>
    {#if view.workshop_stopped}
      <span class="stopped" data-testid="workshop-stopped">Stopped: nothing launches until you resume.</span>
      <button type="button" data-testid="resume" onclick={() => (resumeOpen = true)} disabled={resumeOpen}>Resume</button>
    {:else}
      <button
        type="button"
        data-testid="stop-all"
        disabled={workshop.stopping}
        title="Stop every workshop run and the relay; nothing launches until you resume"
        onclick={() => void workshop.stopAll()}
      >
        <StableLabel labels={STOP_ALL_TEXT} current={workshop.stopping ? "stopping" : "ready"} />
      </button>
    {/if}
  </div>
  {#if workshop.stopped}
    <p class="muted" data-testid="stop-answer">
      Stopped {workshop.stopped.containers} container{workshop.stopped.containers === 1 ? "" : "s"} and
      {workshop.stopped.runs} run{workshop.stopped.runs === 1 ? "" : "s"} in {seconds(workshop.stopped.elapsed_ms)} s{#if !workshop.stopped.engine_reached}<span
          class="fault"
          data-testid="stop-unreached">; Docker did not answer: containers may still run</span
        >{/if}.
    </p>
  {:else if workshop.stopError}
    <p class="fault">{workshop.stopError}</p>
  {/if}
  {#if workshop.resumed}
    <p class="muted" data-testid="resume-answer">
      Resumed: {workshop.resumed.collected} to collect, {workshop.resumed.removed} to remove{workshop.resumed.relay_resumed
        ? ", the relay resumed"
        : ""}.
    </p>
  {/if}
  {#if resumeOpen}
    <ResumeDialog kept={view.runs.kept} onClose={() => (resumeOpen = false)} />
  {/if}

  <h4>Live</h4>
  {#if view.runs.live.length === 0}
    <p class="muted" data-testid="runs-live-none">No run is live.</p>
  {:else}
    <ul data-testid="runs-live">
      {#each view.runs.live as run (run.id)}
        <li data-testid="run-live" data-run={run.id} data-state={run.state}>
          <span class="name">{run.name}</span>
          <span>{run.project}</span>
          <span>{RUN_STATE_TEXT[run.state]}</span>
          <span class="muted">started {ago(run.started_at_ms, nowMs)}</span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if view.runs.kept.length > 0}
    <h4>Kept by Stop all</h4>
    <ul data-testid="runs-kept">
      {#each view.runs.kept as run (run.id)}
        <li data-testid="run-kept" data-run={run.id}>
          <span class="name">{run.name}</span>
          <span>{run.project}</span>
          <span class="muted">kept {ago(run.state_at_ms, nowMs)}</span>
          <button type="button" data-testid="remove-run" disabled={workshop.removing[run.id]} onclick={() => void workshop.removeRun(run.id)}
            >Remove</button
          >
          {#if workshop.removed[run.id]}<span class="muted" data-testid="remove-answer">{removedText(workshop.removed[run.id]!)}</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if orphans.length > 0}
    <h4>Orphans</h4>
    <ul data-testid="runs-orphans">
      {#each orphans as orphan (orphan.runId ?? "")}
        <li data-testid="run-orphan" data-run={orphan.runId ?? ""}>
          <code>{orphan.names.join(", ")}</code>
          {#if orphan.runId !== null}
            {@const runId = orphan.runId}
            <button type="button" data-testid="remove-run" disabled={workshop.removing[runId]} onclick={() => void workshop.removeRun(runId)}
              >Remove</button
            >
            {#if workshop.removed[runId]}<span class="muted" data-testid="remove-answer">{removedText(workshop.removed[runId]!)}</span>{/if}
          {:else}
            <span class="muted">no run label: remove it with Docker</span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px 10px;
  }
  h3 {
    margin: 8px 0 4px;
    font-size: 14px;
  }
  h4 {
    margin: 8px 0 2px;
    font-size: 13px;
    color: var(--color-text-muted);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    padding: 4px 0;
    border-top: 1px solid var(--color-border);
    font-size: 13px;
  }
  .name {
    font-weight: 600;
  }
  p {
    margin: 4px 0;
    font-size: 13px;
  }
  button {
    min-height: var(--target-min);
    padding: 0 12px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
    overflow-wrap: anywhere;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .stopped,
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
