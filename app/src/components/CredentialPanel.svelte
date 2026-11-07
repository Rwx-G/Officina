<script lang="ts">
  // The floor's credential and network (AD-105, D-607, D-611): its kind
  // and state, never a value, and what to do about it. For `login`, Log in
  // for the workshop creates the login volume and shows the command the
  // author runs himself, and Check login runs the auth probe once; each
  // starts a container or creates a volume, so each acts on a pointer
  // click only (D-28, D-1243). For the token kinds, the `setup-token` step
  // and the floor's `cmdkey` line. Copy only copies a text the view named,
  // so the keyboard reaches it. Then each allow-list entry's route.
  import type { NetworkView } from "../lib/generated/NetworkView";
  import type { WorkshopCredential } from "../lib/generated/WorkshopCredential";
  import { isPointerClick } from "../lib/pointer";
  import { workshop } from "../lib/workshop.svelte";
  import { CREDENTIAL_KIND_TEXT, CREDENTIAL_STATE_TEXT, LOGIN_REFUSAL_TEXT, PROXY_SOURCE_TEXT } from "../lib/workshop-text";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    credential: WorkshopCredential | null;
    network: NetworkView | null;
  }
  let { credential, network }: Props = $props();

  const COPY_LABELS = { ready: "Copy", copied: "Copied" } as const;
  const COPIED_VISIBLE_MS = 2_000;
  let copied = $state<string | null>(null);

  let action = $derived(credential?.action ?? null);
  /** The login command: Log in's answer, else the action the state gives. */
  let loginCommand = $derived(
    workshop.login?.status === "ready" ? workshop.login.detail : action?.kind === "login" ? action.command : null,
  );

  async function copy(which: string, text: string): Promise<void> {
    await navigator.clipboard.writeText(text);
    copied = which;
    setTimeout(() => {
      if (copied === which) copied = null;
    }, COPIED_VISIBLE_MS);
  }

  function logIn(event: MouseEvent): void {
    if (!isPointerClick(event)) return;
    void workshop.prepareLogin();
  }

  function check(event: MouseEvent): void {
    if (!isPointerClick(event)) return;
    void workshop.checkLogin();
  }

  function noKeyStarts(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") event.preventDefault();
  }
</script>

<div class="part" data-testid="credential-panel">
  <h3>Credential</h3>
  {#if credential === null}
    <p class="muted" data-testid="credential-none">Not read yet.</p>
  {:else}
    <p data-testid="credential-state" data-kind={credential.kind} data-state={credential.state}>
      {CREDENTIAL_KIND_TEXT[credential.kind]}, {CREDENTIAL_STATE_TEXT[credential.state]}
    </p>
    {#if credential.kind === "login"}
      <div class="row">
        <button type="button" data-testid="log-in" onclick={logIn} onkeydown={noKeyStarts}>Log in for the workshop</button>
        <button type="button" data-testid="check-login" disabled={workshop.checkingLogin} onclick={check} onkeydown={noKeyStarts}
          >Check login</button
        >
      </div>
      {#if loginCommand !== null}
        {@const command = loginCommand}
        <p class="action" data-testid="credential-action" data-kind="login">
          Run this in a terminal, then log in: <code class="text" data-testid="login-command">{command}</code>
          <button type="button" data-testid="credential-copy" onclick={() => void copy("login", command)}>
            <StableLabel labels={COPY_LABELS} current={copied === "login" ? "copied" : "ready"} />
          </button>
        </p>
      {/if}
      {#if workshop.login?.status === "refused"}
        <p class="fault" data-testid="login-refused" data-refusal={workshop.login.detail}>
          {LOGIN_REFUSAL_TEXT[workshop.login.detail]}
        </p>
      {/if}
      {#if workshop.loginCheck?.status === "checked"}
        {@const found = workshop.loginCheck.detail}
        <p data-testid="login-check" data-state={found.state}>
          {#if found.unreadable}
            The probe's answer did not read: the login is taken as missing.
          {:else if found.logged_in}
            Logged in{found.auth_method ? ` (${found.auth_method})` : ""}; the state is {CREDENTIAL_STATE_TEXT[found.state]}.
          {:else}
            Not logged in: run the login command.
          {/if}
        </p>
      {:else if workshop.loginCheck?.status === "refused"}
        <p class="fault" data-testid="login-refused" data-refusal={workshop.loginCheck.detail}>
          {LOGIN_REFUSAL_TEXT[workshop.loginCheck.detail]}
        </p>
      {/if}
      {#if workshop.loginError}
        <p class="fault">{workshop.loginError}</p>
      {/if}
    {:else if action?.kind === "setup_token"}
      <p class="action" data-testid="credential-action" data-kind="setup_token">
        Run <code class="text">{action.step}</code>, then store its token:
        <code class="text" data-testid="cmdkey-line">{action.cmdkey_line}</code>
        <button type="button" data-testid="credential-copy" onclick={() => void copy("cmdkey", action.cmdkey_line)}>
          <StableLabel labels={COPY_LABELS} current={copied === "cmdkey" ? "copied" : "ready"} />
        </button>
      </p>
    {/if}
  {/if}

  <h3>Network</h3>
  {#if network === null}
    <p class="muted" data-testid="network-none">Not read yet.</p>
  {:else}
    <p data-testid="network-source">
      <code>upstream_proxy = "{network.setting}"</code>{#if network.source}, routes from {PROXY_SOURCE_TEXT[network.source]}{/if}.
    </p>
    {#if network.error}
      <p class="fault" data-testid="network-error">{network.error}</p>
    {/if}
    {#if network.routes.length > 0}
      <ul data-testid="network-routes">
        {#each network.routes as route (route.entry)}
          <li data-testid="network-route"><code>{route.entry}</code> <span class="muted">{route.route}</span></li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  h3 {
    margin: 8px 0 4px;
    font-size: 14px;
  }
  p {
    margin: 4px 0;
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .action {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
  }
  ul {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    padding: 1px 8px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-border);
    font-size: 12.5px;
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
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
