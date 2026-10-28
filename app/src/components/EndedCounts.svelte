<script lang="ts">
  // To resume and Done, counted in the header in every view and mode (story
  // 4.15, D-541; D-472 put them under the scene): ended sessions leave the
  // scene, so each count opens Board mode on its column. Fixed controls
  // (D-517): each keeps the width of a three-digit count, so a count that
  // changes moves nothing beside it; it reads counts only, so it changes
  // only when they do.
  import { hours } from "../lib/time";
  import StableLabel from "./StableLabel.svelte";

  export type EndedColumn = "to_resume" | "done";

  interface Props {
    toResume: number;
    done: number;
    /** The board's Done window, worded as the Board's hints word it. */
    doneWindowMs: number;
    onOpen: (column: EndedColumn) => void;
  }
  let { toResume, done, doneWindowMs, onOpen }: Props = $props();

  /** The widest count each label keeps room for: a To resume or Done
   *  column past 99 cards is no list to read anyway. */
  const ROOM = 88;

  type Label = Record<"shown" | "room", string>;
  let toResumeLabel = $derived<Label>({ shown: `${toResume} to resume`, room: `${ROOM} to resume` });
  let doneLabel = $derived<Label>({ shown: `${done} done`, room: `${ROOM} done` });
</script>

<nav class="ended" aria-label="Ended sessions" data-testid="ended-counts">
  <button
    type="button"
    class="count"
    data-testid="ended-to-resume"
    title="Sessions ended or interrupted in the last {hours(doneWindowMs)}: open them in Board mode"
    onclick={() => onOpen("to_resume")}
  >
    <StableLabel labels={toResumeLabel} current="shown" />
  </button>
  <button
    type="button"
    class="count"
    data-testid="ended-done"
    title="Sessions done in the last {hours(doneWindowMs)}: open them in Board mode"
    onclick={() => onOpen("done")}
  >
    <StableLabel labels={doneLabel} current="shown" />
  </button>
</nav>

<style>
  .ended {
    display: flex;
    gap: 6px;
  }
  .count {
    min-height: var(--target-min);
    padding: 0 10px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
  }
</style>
