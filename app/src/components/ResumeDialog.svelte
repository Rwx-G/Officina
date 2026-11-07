<script lang="ts">
  // Resume (AD-116): each kept run to collect, the default, or remove, and
  // the relay resumed too, checked by default. Resume launches what the
  // stop held back, so it acts on a pointer click only (D-28); Cancel
  // only closes.
  import type { KeptChoice } from "../lib/generated/KeptChoice";
  import type { WorkshopRunView } from "../lib/generated/WorkshopRunView";
  import { isPointerClick } from "../lib/pointer";
  import { workshop } from "../lib/workshop.svelte";

  interface Props {
    kept: WorkshopRunView[];
    onClose: () => void;
  }
  let { kept, onClose }: Props = $props();

  let choices = $state<Record<string, KeptChoice>>({});
  let resumeRelay = $state(true);

  const choiceOf = (id: string): KeptChoice => choices[id] ?? "collect";

  async function resume(event: MouseEvent): Promise<void> {
    if (!isPointerClick(event)) return;
    const sent = kept.map((run) => ({ run_id: run.id, choice: choiceOf(run.id) }));
    if (await workshop.resume(sent, resumeRelay)) onClose();
  }

  function noKeyStarts(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") event.preventDefault();
  }
</script>

<div class="dialog" role="dialog" aria-labelledby="resume-title" data-testid="resume-dialog">
  <h3 id="resume-title">Resume the workshop</h3>
  {#if kept.length === 0}
    <p class="muted">No run was kept.</p>
  {:else}
    <ul>
      {#each kept as run (run.id)}
        <li data-testid="resume-run" data-run={run.id}>
          <span class="name">{run.name}</span>
          <label>
            <input
              type="radio"
              name="resume-{run.id}"
              data-testid="resume-collect"
              checked={choiceOf(run.id) === "collect"}
              onchange={() => (choices[run.id] = "collect")}
            />
            Collect
          </label>
          <label>
            <input
              type="radio"
              name="resume-{run.id}"
              data-testid="resume-remove"
              checked={choiceOf(run.id) === "remove"}
              onchange={() => (choices[run.id] = "remove")}
            />
            Remove
          </label>
        </li>
      {/each}
    </ul>
  {/if}
  <label class="relay">
    <input type="checkbox" data-testid="resume-relay" bind:checked={resumeRelay} />
    Resume the relay too
  </label>
  <div class="actions">
    <button type="button" data-testid="resume-confirm" disabled={workshop.resuming} onclick={resume} onkeydown={noKeyStarts}>
      Resume
    </button>
    <button type="button" data-testid="resume-cancel" onclick={onClose}>Cancel</button>
  </div>
  {#if workshop.resumeError}
    <p class="fault" role="alert">{workshop.resumeError}</p>
  {/if}
</div>

<style>
  .dialog {
    margin: 6px 0;
    padding: 8px 12px;
    border: 1px solid var(--color-text);
    border-radius: var(--radius-control);
  }
  h3 {
    margin: 4px 0;
    font-size: 14px;
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
    gap: 4px 14px;
    padding: 4px 0;
    font-size: 13px;
  }
  .name {
    font-weight: 600;
    min-width: 12em;
  }
  .relay {
    display: block;
    margin: 6px 0;
    font-size: 13px;
  }
  .actions {
    display: flex;
    gap: 8px;
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
  p {
    margin: 4px 0;
    font-size: 13px;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
