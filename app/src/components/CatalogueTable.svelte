<script lang="ts">
  // The approved catalogue (D-24), what `officina run` serves: one row per
  // action with its class, the projects that list it, what it starts, its
  // parameters and limits; then the projects and the guarded programs.
  // Catalogue text comes from a floor a session may have edited: it is
  // rendered as text only.
  import type { CanonicalCatalogue } from "../lib/generated/CanonicalCatalogue";
  import type { CanonicalParam } from "../lib/generated/CanonicalParam";
  import ActionRuns from "./ActionRuns.svelte";

  interface Props {
    catalogue: CanonicalCatalogue;
  }
  let { catalogue }: Props = $props();

  let actions = $derived(Object.entries(catalogue.actions));
  let projects = $derived(Object.entries(catalogue.projects));

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
        <td data-testid="catalogue-class">{action.class}{action.check ? ` (after ${action.check})` : ""}</td>
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

<p class="muted" data-testid="catalogue-guarded-programs">
  Guarded programs: {catalogue.guarded_programs.join(", ") || "(none)"}
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
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
</style>
