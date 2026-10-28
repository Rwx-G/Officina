<script lang="ts" module>
  export interface RegisterRow {
    key: string;
    name: string;
    /** The board number, `null` for an approval-only wait. */
    number: number | null;
    alert: boolean;
  }
</script>

<script lang="ts">
  // The decision office's register (D-103, D-524): who waits, in board
  // order, `REGISTER.rows` rows, then how many more. Names render as text.
  import { REGISTER } from "../../lib/scene/geometry";
  import { SIGN_WORDS, registerMore } from "../../lib/scene/text";

  interface Props {
    rows: RegisterRow[];
  }
  let { rows }: Props = $props();

  let shown = $derived(rows.slice(0, REGISTER.rows));
  let hidden = $derived(rows.length - shown.length);
</script>

<div class="register" data-testid="register" style:--register-head={REGISTER.head} style:--register-row={REGISTER.rowH}>
  <div class="head">{SIGN_WORDS.register}</div>
  {#each shown as row (row.key)}
    <div class="row" data-register-row={row.key}>
      <span class="ticket" class:alert={row.alert}>{row.number ?? ""}</span>
      <span class="name">{row.name}</span>
    </div>
  {/each}
  {#if hidden > 0}
    <div class="row more">{registerMore(hidden)}</div>
  {/if}
</div>

<style>
  .register {
    box-sizing: border-box;
    width: 100%;
    padding: 0 6px 6px 9px;
    border: 1.5px solid var(--color-attention-border);
    border-radius: var(--radius-surface);
    background: var(--color-surface);
    color: var(--color-text);
    font-weight: 700;
    font-size: 12px;
    line-height: 1;
  }
  .head {
    height: calc(var(--register-head) * 1px);
    line-height: calc(var(--register-head) * 1px);
    font-size: 11.5px;
    font-weight: 800;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 7px;
    height: calc(var(--register-row) * 1px);
  }
  .ticket {
    flex: none;
    display: grid;
    place-items: center;
    width: 15px;
    height: 15px;
    border-radius: var(--radius-round);
    background: var(--color-attention);
    color: var(--color-attention-text);
    font-size: 10.5px;
    font-weight: 800;
  }
  .ticket.alert {
    background: var(--color-alert);
    color: var(--color-on-alert);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .more {
    font-size: 11.5px;
  }
</style>
