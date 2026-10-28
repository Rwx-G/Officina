<script lang="ts">
  // A pending catalogue change (AD-24, AD-35): its diff per action and per
  // field and the Approve button, shown as the Board view's banner and in
  // the Relay view. Catalogue text comes from a floor a session may have
  // edited: it is rendered as text only.
  import { fieldLines, isPointerClick, shown, type RelayStore } from "../lib/relay.svelte";
  import type { CanonicalAction } from "../lib/generated/CanonicalAction";
  import type { CanonicalParam } from "../lib/generated/CanonicalParam";
  import type { FieldChange } from "../lib/generated/FieldChange";
  import ActionRuns from "./ActionRuns.svelte";

  interface Props {
    store: RelayStore;
    /** The banner lists the floor's issues; the Relay view has its own
     *  section for them. */
    showIssues?: boolean;
  }
  let { store, showIssues = true }: Props = $props();

  let relay = $derived(store.relay);
  let pending = $derived(store.pending);
  let diff = $derived(pending?.diff);

  function rule(param: CanonicalParam | undefined): string {
    if (!param) return "";
    return "values" in param ? `one of ${param.values.join(", ")}` : `the whole value matches ${param.pattern}`;
  }

  function approve(event: MouseEvent, hash: string): void {
    if (isPointerClick(event)) void store.approve(hash);
  }
</script>

{#snippet fieldChanges(fields: FieldChange[])}
  <ul>
    {#each fields as field (field.field)}
      <li data-testid="field-change" data-field={field.field}>
        {#each fieldLines(field) as line (line.label)}
          <div>{line.label}: <code>{line.before}</code> to <code>{line.after}</code></div>
        {/each}
      </li>
    {/each}
  </ul>
{/snippet}

{#snippet actionDetail(action: CanonicalAction)}
  <dl class="detail">
    <dt>class</dt>
    <dd>{action.class}{action.check ? ` (after ${action.check})` : ""}</dd>
    <dt>runs</dt>
    <dd><ActionRuns {action} /></dd>
    {#if Object.keys(action.env).length > 0}
      <dt>env</dt>
      <dd>
        {#each Object.entries(action.env) as [key, value] (key)}<code class="chip">{key}={value}</code>{/each}
      </dd>
    {/if}
    {#each Object.entries(action.params) as [name, param] (name)}
      <dt>{name}</dt>
      <dd>{rule(param)}</dd>
    {/each}
    {#if action.description}
      <dt>description</dt>
      <dd>{action.description}</dd>
    {/if}
    <dt>limits</dt>
    <dd>{action.timeout_s} s, {action.max_output_kib} KiB of output</dd>
  </dl>
{/snippet}

{#if relay && pending && diff}
  <section class="banner pending" data-testid="catalogue-banner" aria-labelledby="catalogue-title">
    <h2 id="catalogue-title">
      {relay.approved_hash === null ? "A relay catalogue waits for your approval" : "The relay catalogue changed"}
    </h2>
    <p class="lead">
      Read from <span data-testid="catalogue-floor">{relay.floor}</span>. Nothing new runs through the relay until you
      approve what is shown here.
    </p>
    {#if relay.snapshot_error}
      <p class="lead" data-testid="snapshot-error">The approved catalogue cannot be read ({relay.snapshot_error}).</p>
    {/if}
    {#if store.changedAgain}
      <p class="refusal" role="alert" data-testid="approval-refused">
        The catalogue changed again after it was shown, so nothing was approved. Below is the new diff: review it, then
        approve it.
      </p>
    {:else if store.refused}
      <p class="refusal" role="alert" data-testid="approval-refused">Nothing was approved: {store.refused.message}</p>
    {/if}

    {#if diff.format || diff.floor_path}
      <ul class="changes">
        {#each [diff.format, diff.floor_path] as change, i (i)}
          {#if change}
            <li data-testid="catalogue-field">{change.field}: {shown(change.before)} to {shown(change.after)}</li>
          {/if}
        {/each}
      </ul>
    {/if}

    {#if diff.guarded_programs.added.length > 0 || diff.guarded_programs.removed.length > 0}
      <h3>Guarded programs</h3>
      <ul class="changes" data-testid="catalogue-guarded">
        {#each diff.guarded_programs.added as program (program)}<li>added <code>{program}</code></li>{/each}
        {#each diff.guarded_programs.removed as program (program)}<li>removed <code>{program}</code></li>{/each}
      </ul>
    {/if}

    {#if diff.actions.added.length + diff.actions.removed.length + diff.actions.changed.length > 0}
      <h3>Actions</h3>
      <ul class="changes">
        {#each diff.actions.added as name (name)}
          <li data-testid="action-added" data-name={name}>
            <strong>{name}</strong> added
            {#if pending.current.actions[name]}{@render actionDetail(pending.current.actions[name])}{/if}
          </li>
        {/each}
        {#each diff.actions.changed as entry (entry.name)}
          <li data-testid="action-changed" data-name={entry.name}>
            <strong>{entry.name}</strong> changed
            {@render fieldChanges(entry.fields)}
          </li>
        {/each}
        {#each diff.actions.removed as name (name)}
          <li data-testid="action-removed" data-name={name}><strong>{name}</strong> removed</li>
        {/each}
      </ul>
    {/if}

    {#if diff.projects.added.length + diff.projects.removed.length + diff.projects.changed.length > 0}
      <h3>Projects</h3>
      <ul class="changes">
        {#each diff.projects.added as name (name)}
          {@const project = pending.current.projects[name]}
          <li data-testid="project-added" data-name={name}>
            <strong>{name}</strong> added{#if project}: {project.autonomy}, {project.path}, actions
              {project.actions.join(", ") || "(none)"}{/if}
          </li>
        {/each}
        {#each diff.projects.changed as entry (entry.name)}
          <li data-testid="project-changed" data-name={entry.name}>
            <strong>{entry.name}</strong> changed
            {@render fieldChanges(entry.fields)}
          </li>
        {/each}
        {#each diff.projects.removed as name (name)}
          <li data-testid="project-removed" data-name={name}><strong>{name}</strong> removed</li>
        {/each}
      </ul>
    {/if}

    {#if showIssues && relay.issues.length > 0}
      <h3>Not part of the catalogue</h3>
      <ul class="changes" data-testid="catalogue-issues">
        {#each relay.issues as issue, i (i)}
          <li>{issue.name}{issue.line !== null ? ` (line ${issue.line})` : ""}: {issue.message}</li>
        {/each}
      </ul>
    {/if}

    <button
      class="approve"
      data-testid="approve-catalogue"
      disabled={store.approving}
      onclick={(event) => approve(event, pending.hash)}
      title="A click approves exactly the catalogue shown; no key does"
    >
      Approve this catalogue
    </button>
  </section>
{/if}

<style>
  .banner {
    margin: 0 20px 12px;
    color: var(--color-text);
    border-radius: 16px;
  }
  .pending {
    background: var(--color-surface);
    border: 2px solid var(--color-attention-border);
    padding: 14px 18px;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 17px;
  }
  h3 {
    margin: 12px 0 4px;
    font-size: 14px;
  }
  .lead {
    margin: 0 0 6px;
    color: var(--color-text-muted);
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .refusal {
    margin: 6px 0;
    padding: 8px 12px;
    border-radius: 12px;
    background: var(--color-stopped-surface);
    border: 1px solid var(--color-stopped);
    font-weight: 600;
  }
  .changes {
    margin: 0;
    padding-left: 18px;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .changes li {
    margin: 3px 0;
  }
  .detail {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 10px;
    margin: 4px 0 8px;
  }
  .detail dt {
    color: var(--color-text-muted);
  }
  .detail dd {
    margin: 0;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .chip {
    display: inline-block;
    margin: 1px 4px 1px 0;
    padding: 1px 6px;
    border-radius: 6px;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
  }
  .approve {
    margin-top: 12px;
    min-height: var(--target-min);
    padding: 0 22px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-attention-border);
    background: var(--color-attention);
    color: var(--color-attention-text);
    font-weight: 700;
    cursor: pointer;
  }
  .approve:disabled {
    opacity: 0.6;
    cursor: progress;
  }
</style>
