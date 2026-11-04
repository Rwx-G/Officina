<script lang="ts">
  // What a workshop card holds beside a host card's (AD-107, AD-122, FR-6):
  // the base from its closed list, what the run will not carry, the
  // allow-list, the read-only mounts, the image, the limits and every
  // readiness cause, each with what to do about it. Run escape battery
  // starts containers, so it acts on a pointer click only (D-28); Copy only
  // copies text the view named (D-607), so the keyboard reaches it.
  import type { BaseChoice } from "../lib/generated/BaseChoice";
  import type { CredentialAction } from "../lib/generated/CredentialAction";
  import type { WorkshopCardFacts } from "../lib/generated/WorkshopCardFacts";
  import { LAUNCH_REFUSAL_TEXT } from "../lib/orchestrator-text";
  import { isPointerClick } from "../lib/pointer";
  import { workshop } from "../lib/workshop.svelte";
  import { MOUNT_STATE_TEXT, baseText, dollars, sameBase } from "../lib/workshop-text";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    cardId: string;
    facts: WorkshopCardFacts;
    /** No base change while the card is launched, launching or edited. */
    locked: boolean;
    /** Issues the card again from `base` (AD-78's `Base` edit). */
    onBase: (base: BaseChoice) => void;
  }
  let { cardId, facts, locked, onBase }: Props = $props();

  const COPY_LABELS = { ready: "Copy", copied: "Copied" } as const;
  const COPIED_VISIBLE_MS = 2_000;

  let selected = $derived(Math.max(0, facts.bases.findIndex((option) => sameBase(option.choice, facts.base))));
  let action = $derived<CredentialAction | null>(workshop.view?.credential?.action ?? null);
  let copied = $state<string | null>(null);

  function chose(event: Event): void {
    const index = Number((event.currentTarget as HTMLSelectElement).value);
    const option = facts.bases[index];
    if (option && !sameBase(option.choice, facts.base)) onBase(option.choice);
  }

  async function copy(which: string, text: string): Promise<void> {
    await navigator.clipboard.writeText(text);
    copied = which;
    setTimeout(() => {
      if (copied === which) copied = null;
    }, COPIED_VISIBLE_MS);
  }

  function battery(event: MouseEvent): void {
    if (!isPointerClick(event)) return;
    void workshop.runBattery(facts.image.tag);
  }

  function noKeyStarts(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") event.preventDefault();
  }
</script>

<div class="facts" data-testid="workshop-facts">
  <label class="row">
    <span class="label">Base</span>
    <select data-testid="workshop-base" value={String(selected)} disabled={locked} onchange={chose}>
      {#each facts.bases as option, i (i)}
        <option value={String(i)} data-testid="workshop-base-option">
          {baseText(option.choice)}{option.tip ? ` @ ${option.tip.slice(0, 12)}` : " (does not resolve)"}
        </option>
      {/each}
    </select>
  </label>
  <p data-testid="workshop-uncommitted">
    <span class="label">Uncommitted</span>
    {facts.uncommitted} change{facts.uncommitted === 1 ? "" : "s"} in the project, not carried: the run starts from the base's commit.
  </p>
  <div class="row">
    <span class="label">Allowed hosts</span>
    {#if facts.allow.length === 0}
      <span class="muted" data-testid="workshop-allow-none">none</span>
    {:else}
      <ul class="chips" data-testid="workshop-allow">
        {#each facts.allow as entry, i (i)}
          <li class="chip">{entry.host}{#if entry.route}<span class="muted"> via {entry.route}</span>{/if}</li>
        {/each}
      </ul>
    {/if}
  </div>
  <div class="row">
    <span class="label">Mounts</span>
    {#if facts.mounts.length === 0}
      <span class="muted">none</span>
    {:else}
      <ul class="chips" data-testid="workshop-mounts">
        {#each facts.mounts as mount, i (i)}
          <li class="chip" data-testid="workshop-mount" data-state={mount.state}>
            {mount.path} <span class="muted">{MOUNT_STATE_TEXT[mount.state]}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  <p data-testid="workshop-image">
    <span class="label">Image</span>
    <code>{facts.image.tag}</code>
    {#if facts.image.short_id}<code class="muted">{facts.image.short_id}</code>{:else}<span class="muted">not checked</span>{/if}
  </p>
  <p data-testid="workshop-limits">
    <span class="label">Limits</span>
    {facts.max_minutes} minutes{#if facts.max_usd_cents !== null}, a budget of {dollars(facts.max_usd_cents)} checked after each turn{/if}.
  </p>
  {#if facts.readiness.length > 0}
    <ul class="causes" data-testid="workshop-readiness" aria-label="Why the card cannot launch">
      {#each facts.readiness as cause (cause.refusal)}
        <li data-testid="workshop-cause" data-cause={cause.refusal}>
          <span>{LAUNCH_REFUSAL_TEXT[cause.refusal]}{#if cause.detail}<span class="muted"> ({cause.detail})</span>{/if}</span>
          {#if cause.refusal === "battery_stale"}
            <button type="button" data-testid="workshop-battery" onclick={battery} onkeydown={noKeyStarts}>Run escape battery</button>
            {#if workshop.battery?.outcome === "queued"}
              <span class="muted" data-testid="workshop-battery-queued">Queued: the card follows its outcome.</span>
            {:else if workshop.battery?.outcome === "refused"}
              <span class="fault" data-testid="workshop-battery-refused">Refused: {workshop.battery.detail}.</span>
            {:else if workshop.batteryError}
              <span class="fault">{workshop.batteryError}</span>
            {/if}
          {:else if cause.refusal === "credential_expired" && action}
            {#if action.kind === "login"}
              <span class="action" data-testid="workshop-credential-action" data-kind="login">
                Login needed: <code class="text">{action.command}</code>
                <button type="button" data-testid="workshop-copy" onclick={() => void copy(`${cardId}:login`, action.command)}>
                  <StableLabel labels={COPY_LABELS} current={copied === `${cardId}:login` ? "copied" : "ready"} />
                </button>
              </span>
            {:else}
              <span class="action" data-testid="workshop-credential-action" data-kind="setup_token">
                <span class="text">{action.step}</span>
                <code class="text">{action.cmdkey_line}</code>
                <button type="button" data-testid="workshop-copy" onclick={() => void copy(`${cardId}:cmdkey`, action.cmdkey_line)}>
                  <StableLabel labels={COPY_LABELS} current={copied === `${cardId}:cmdkey` ? "copied" : "ready"} />
                </button>
              </span>
            {/if}
          {:else if cause.refusal === "proxy_unresolved" || cause.refusal === "proxy_unsupported"}
            <span class="muted" data-testid="workshop-doctor">Run <code>officina doctor</code> to see each host's route.</span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .facts {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
    font-size: 13.5px;
  }
  p {
    margin: 0;
    font-size: 13.5px;
    overflow-wrap: anywhere;
  }
  .label {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--color-text-muted);
    margin-right: 6px;
  }
  select {
    min-height: var(--target-min);
    max-width: 100%;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .chip,
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .chip {
    padding: 1px 7px;
    border-radius: var(--radius-surface);
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    overflow-wrap: anywhere;
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    unicode-bidi: isolate;
  }
  .causes {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding-left: 18px;
    font-size: 13.5px;
  }
  .causes li > * + * {
    margin-left: 8px;
  }
  .action {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
  }
  button {
    min-height: var(--target-min);
    padding: 0 14px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    color: var(--color-text);
    font-weight: 600;
    cursor: pointer;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
