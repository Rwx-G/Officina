<script lang="ts" module>
  /** What the author picked: an open subject, or a new subject's name. */
  export type PickerChoice = { subject: number } | { name: string };
</script>

<script lang="ts">
  // The subject to file sessions under (AD-55): the open subjects of the
  // sessions' project first, then the others, and a new name. Subject names
  // render as text.
  import type { SubjectView } from "../lib/generated/SubjectView";

  interface Props {
    subjects: SubjectView[];
    /** The sessions' project, whose subjects come first. */
    project: string | null;
    label: string;
    disabled?: boolean;
    onPick: (choice: PickerChoice) => void;
  }
  let { subjects, project, label, disabled = false, onPick }: Props = $props();

  let name = $state("");
  let ordered = $derived([
    ...subjects.filter((s) => s.project === project),
    ...subjects.filter((s) => s.project !== project),
  ]);

  function create(event: SubmitEvent): void {
    event.preventDefault();
    if (name.trim() === "") return;
    onPick({ name });
  }
</script>

<div class="picker" role="group" aria-label={label} data-testid="subject-picker">
  {#if ordered.length > 0}
    <ul>
      {#each ordered as subject (subject.id)}
        <li>
          <button
            type="button"
            class="option"
            class:own={subject.project === project}
            data-testid="pick-subject"
            data-subject={subject.id}
            {disabled}
            onclick={() => onPick({ subject: subject.id })}
          >
            <span class="name">{subject.name}</span>
            <span class="project">{subject.project}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  <form onsubmit={create}>
    <input bind:value={name} placeholder="New subject" aria-label="New subject name" data-testid="picker-new-name" />
    <button type="submit" class="option" disabled={disabled || name.trim() === ""} data-testid="picker-create">
      Create
    </button>
  </form>
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  ul {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  form {
    display: flex;
    gap: 6px;
  }
  input {
    min-height: 36px;
    padding: 0 12px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-border);
    font: inherit;
  }
  .option {
    min-height: 36px;
    padding: 0 14px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
  .option.own {
    border-width: 2px;
  }
  .project {
    margin-left: 6px;
    font-weight: 400;
    color: var(--color-text-muted);
  }
  .name {
    overflow-wrap: anywhere;
  }
</style>
