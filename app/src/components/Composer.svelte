<script lang="ts">
  // The composer (AD-93, FR-4): a work order from an approved template, its
  // project and one value per parameter, each picked from its list. No
  // value can be typed: the only text field is the subject picker's new
  // name (AD-55), which names a subject, never a value. Each choice asks
  // core for a card, or edits the open one (AD-78); the card or the typed
  // refusal is shown. Templates and their descriptions come from the
  // approved floor, rendered as text. While a card is on screen a status
  // line under the subject picker names it and its subject, so a choice
  // shows its effect where it was made (D-1001); the line holds one line
  // whatever it says, so nothing after it moves (D-516).
  import type { OrchestratorStore } from "../lib/orchestrator.svelte";
  import {
    LAUNCH_REFUSAL_TEXT,
    TEMPLATE_CHANGE_TEXT,
    composerStatusText,
    issueRefusalText,
  } from "../lib/orchestrator-text";
  import SubjectPicker, { type PickerChoice } from "./SubjectPicker.svelte";

  interface Props {
    store: OrchestratorStore;
  }
  let { store }: Props = $props();

  let view = $derived(store.view);
  let template = $derived(store.template);
  let card = $derived(store.card);
  let busy = $derived(store.composing || store.launching);
  let fault = $derived.by(() => {
    const f = store.fault;
    if (!f) return null;
    switch (f.kind) {
      case "issue":
        return issueRefusalText(f.refusal);
      case "launch":
        return LAUNCH_REFUSAL_TEXT[f.refusal];
      case "error":
        return `The request failed: ${f.message}`;
    }
  });

  function pickSubject(choice: PickerChoice): void {
    void store.chooseSubject("subject" in choice ? { kind: "existing", value: choice.subject } : { kind: "new", value: choice.name });
  }
</script>

<section class="block" data-testid="composer" aria-labelledby="composer-title">
  <h2 id="composer-title">Compose a work order</h2>
  {#if !view || view.templates.length === 0}
    <p class="muted" data-testid="templates-none">
      No template is approved: add a <code>[[template]]</code> to the relay floor and approve the catalogue in the Relay
      view.
    </p>
  {:else}
    <div class="pickers">
      <label>
        <span class="label">Template</span>
        <select
          data-testid="composer-template"
          value={store.choices.template}
          disabled={busy}
          onchange={(event) => void store.chooseTemplate(event.currentTarget.value)}
        >
          <option value="" disabled hidden>Choose a template</option>
          {#each view.templates as option (option.name)}
            <option value={option.name} data-testid="template-option">{option.name}: {option.description}</option>
          {/each}
        </select>
      </label>
      {#if template}
        <label>
          <span class="label">Project</span>
          <select
            data-testid="composer-project"
            value={store.choices.project}
            disabled={busy}
            onchange={(event) => void store.chooseProject(event.currentTarget.value)}
          >
            <option value="" disabled hidden>Choose a project</option>
            {#each template.projects as project (project)}
              <option value={project}>{project}</option>
            {/each}
          </select>
        </label>
        {#each Object.entries(template.params) as [name, list] (name)}
          <label>
            <span class="label">{name}</span>
            <select
              data-testid="composer-param"
              data-param={name}
              value={store.choices.values[name] ?? ""}
              disabled={busy}
              onchange={(event) => void store.chooseValue(name, event.currentTarget.value)}
            >
              <option value="" disabled hidden>Choose a value</option>
              {#each list as value (value)}
                <option {value}>{value}</option>
              {/each}
            </select>
          </label>
        {/each}
      {/if}
    </div>
    {#if card}
      <div class="subject">
        <span class="label">File the session under</span>
        <SubjectPicker
          subjects={card.subject_options.map((s) => ({ id: s.id, name: s.name, project: card.project }))}
          project={card.project}
          label="The subject the work order's session is filed under"
          selected={card.subject.kind === "existing" ? card.subject.value : null}
          disabled={busy}
          onPick={pickSubject}
        />
        {#if card.proposal?.suggested_subject}
          <!-- D-152: the proposal's name is a suggestion to type, never preselected. -->
          <span class="muted suggestion" data-testid="composer-suggested-subject">
            Suggested by the proposal: <span class="text">{card.proposal.suggested_subject}</span>
          </span>
        {/if}
        <p class="status" role="status" data-testid="composer-status" title={composerStatusText(card)}>
          {composerStatusText(card)}
        </p>
      </div>
    {/if}
  {/if}
  {#if fault}
    <p class="fault" role="alert" data-testid="composer-refusal">{fault}</p>
  {/if}
  {#if view && view.awaiting_templates.length > 0}
    <h3>Awaiting your approval</h3>
    <ul>
      {#each view.awaiting_templates as awaiting (awaiting.name)}
        <li data-testid="template-awaiting" data-change={awaiting.change}>
          <strong>{awaiting.name}</strong>
          {TEMPLATE_CHANGE_TEXT[awaiting.change]}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .block {
    margin: 0 20px;
    padding: 12px 18px;
    border-radius: var(--radius-surface);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
  }
  h2 {
    margin: 0 0 6px;
    font-size: 16px;
  }
  h3 {
    margin: 10px 0 4px;
    font-size: 14px;
  }
  .pickers {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 14px;
    align-items: flex-end;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .label {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-muted);
  }
  select {
    min-height: var(--target-min);
    min-width: 160px;
    max-width: 100%;
    padding: 0 10px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
  }
  select:disabled {
    cursor: progress;
  }
  .subject {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 10px;
  }
  p,
  ul {
    margin: 6px 0 0;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  ul {
    padding-left: 18px;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .suggestion {
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .text {
    white-space: pre-wrap;
    unicode-bidi: isolate;
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
  /* One line whatever the names: a long one is cut, its whole text in the
     title, so the card below never moves as the line changes. */
  .status {
    margin: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    unicode-bidi: isolate;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
</style>
