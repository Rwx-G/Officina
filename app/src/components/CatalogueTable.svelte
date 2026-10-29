<script lang="ts">
  // The approved catalogue (D-24), what `officina run` serves: one row per
  // action with its class, the projects that list it, what it starts, its
  // parameters and limits; then the templates (AD-76), the projects, the
  // credentials, the guarded programs and the review's model. Catalogue
  // text comes from a floor a session may have edited: it is rendered as
  // text only.
  import { classText, reviewModelText } from "../lib/relay.svelte";
  import type { CanonicalCatalogue } from "../lib/generated/CanonicalCatalogue";
  import type { CanonicalParam } from "../lib/generated/CanonicalParam";
  import type { CredentialView } from "../lib/generated/CredentialView";
  import ActionRuns from "./ActionRuns.svelte";

  interface Props {
    catalogue: CanonicalCatalogue;
    /** The approved credentials with what the window found in the
     *  Credential Manager; never a secret. */
    credentials?: CredentialView[];
  }
  let { catalogue, credentials = [] }: Props = $props();

  function presence(present: boolean | null): string {
    if (present === null) return "unchecked";
    return present ? "present" : "missing";
  }

  let actions = $derived(Object.entries(catalogue.actions));
  let projects = $derived(Object.entries(catalogue.projects));
  let templates = $derived(Object.entries(catalogue.templates));

  function projectsOf(action: string): string[] {
    return projects.filter(([, project]) => project.actions.includes(action)).map(([name]) => name);
  }

  function rule(param: CanonicalParam): string {
    return "values" in param ? `one of ${param.values.join(", ")}` : `matches ${param.pattern}`;
  }
</script>

<table class="catalogue" data-testid="catalogue-table">
  <thead>
    <tr>
      <th scope="col">Action</th>
      <th scope="col">Class</th>
      <th scope="col">Projects</th>
      <th scope="col">Runs</th>
      <th scope="col">Parameters</th>
      <th scope="col">Limits</th>
    </tr>
  </thead>
  <tbody>
    {#each actions as [name, action] (name)}
      <tr data-testid="catalogue-action" data-name={name}>
        <td>
          <strong>{name}</strong>
          {#if action.description}<div class="muted">{action.description}</div>{/if}
        </td>
        <td data-testid="catalogue-class">{classText(action)}</td>
        <td data-testid="catalogue-projects">{projectsOf(name).join(", ") || "(none)"}</td>
        <td><ActionRuns {action} /></td>
        <td>
          {#each Object.entries(action.params) as [param, value] (param)}
            <div>{param}: {rule(value)}</div>
          {/each}
        </td>
        <td>{action.timeout_s} s, {action.max_output_kib} KiB</td>
      </tr>
    {/each}
  </tbody>
</table>
{#if actions.length === 0}
  <p class="muted">The approved catalogue declares no action.</p>
{/if}

{#if templates.length > 0}
  <table class="catalogue" data-testid="catalogue-templates-table">
    <thead>
      <tr>
        <th scope="col">Template</th>
        <th scope="col">Projects</th>
        <th scope="col">Prompt</th>
        <th scope="col">Parameters</th>
        <th scope="col">Model</th>
      </tr>
    </thead>
    <tbody>
      {#each templates as [name, template] (name)}
        <tr data-testid="catalogue-template" data-name={name}>
          <td>
            <strong>{name}</strong>
            <div class="muted" data-testid="template-description">{template.description}</div>
          </td>
          <td data-testid="template-projects">{template.projects.join(", ")}</td>
          <td><div class="prompt" data-testid="template-prompt">{template.prompt}</div></td>
          <td data-testid="template-params">
            {#each Object.entries(template.params) as [param, values] (param)}
              <div data-testid="template-param" data-name={param}>{param}: one of {values.join(", ")}</div>
            {/each}
          </td>
          <td data-testid="template-model">{template.model ?? "default"}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<table class="catalogue projects" data-testid="catalogue-projects-table">
  <thead>
    <tr>
      <th scope="col">Project</th>
      <th scope="col">Folder</th>
      <th scope="col">Autonomy</th>
      <th scope="col">Actions</th>
    </tr>
  </thead>
  <tbody>
    {#each projects as [name, project] (name)}
      <tr data-testid="catalogue-project" data-name={name}>
        <td><strong>{name}</strong></td>
        <td><code>{project.path}</code></td>
        <td>{project.autonomy}</td>
        <td>{project.actions.join(", ") || "(none)"}</td>
      </tr>
    {/each}
  </tbody>
</table>

{#if credentials.length > 0}
  <table class="catalogue" data-testid="catalogue-credential-table">
    <thead>
      <tr>
        <th scope="col">Credential</th>
        <th scope="col">Sent to</th>
        <th scope="col">Actions</th>
        <th scope="col">Credential Manager</th>
      </tr>
    </thead>
    <tbody>
      {#each credentials as credential (credential.name)}
        <tr data-testid="catalogue-credential" data-name={credential.name}>
          <td><strong>{credential.name}</strong></td>
          <td>{credential.hosts.join(", ")}</td>
          <td>{credential.actions.join(", ") || "(none)"}</td>
          <td class="presence {presence(credential.present)}" data-testid="credential-presence">
            {presence(credential.present)}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<p class="muted" data-testid="catalogue-guarded-programs">
  Guarded programs: {catalogue.guarded_programs.join(", ") || "(none)"}
</p>
<p class="muted" data-testid="catalogue-review-model">
  Review model of auto requests: {reviewModelText(catalogue.auto_review.model)}
</p>

<style>
  .catalogue {
    width: 100%;
    border-collapse: collapse;
    font-size: 13.5px;
    margin-bottom: 10px;
  }
  th {
    text-align: left;
    font-weight: 600;
    color: var(--color-text-muted);
    border-bottom: 1px solid var(--color-border);
    padding: 4px 8px;
  }
  td {
    vertical-align: top;
    padding: 6px 8px;
    border-bottom: 1px solid var(--color-border);
    overflow-wrap: anywhere;
  }
  .muted {
    color: var(--color-text-muted);
    font-size: 13px;
  }
  .prompt {
    white-space: pre-wrap;
  }
  .presence.missing {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
  .presence.unchecked {
    color: var(--color-text-muted);
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
</style>
