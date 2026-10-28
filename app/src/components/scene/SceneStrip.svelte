<script lang="ts">
  // The line under the scene (AD-177, D-472): ended sessions leave the
  // scene, so two buttons count them and open Board mode, where they are.
  // Outside the scene element; it reads counts only, so it changes only
  // when they do.
  import { hours } from "../../lib/time";

  interface Props {
    toResume: number;
    done: number;
    /** The board's Done window, worded as the Board's hints word it. */
    doneWindowMs: number;
    onBoard: () => void;
  }
  let { toResume, done, doneWindowMs, onBoard }: Props = $props();
</script>

<nav class="strip" aria-label="Ended sessions" data-testid="scene-strip">
  <button type="button" data-testid="strip-to-resume" onclick={onBoard}>{toResume} to resume</button>
  <button type="button" data-testid="strip-done" onclick={onBoard}>{done} done in {hours(doneWindowMs)}</button>
</nav>

<style>
  .strip {
    display: flex;
    gap: 8px;
  }
  button {
    min-height: var(--target-min);
    padding: 0 16px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
  }
</style>
