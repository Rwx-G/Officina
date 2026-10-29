<script lang="ts" module>
  export interface RegisterRow {
    key: string;
    name: string;
    /** The board number; `null` beyond the ninth and for an approval-only
     *  wait. */
    number: number | null;
    /** An approval-only wait (D-464). */
    approval: boolean;
    alert: boolean;
  }
</script>

<script lang="ts">
  // The register (D-103, D-524; story 4.17, D-555, D-556): how many wait,
  // in its heading, then who, in board order, `REGISTER.rows` rows and how
  // many more. Overlay UI over the scene's top-right corner at a fixed
  // size, never scaled with the floor; the scene's text alternative says
  // the same, so it is hidden from assistive technology. Names render as
  // text.
  import { REGISTER } from "../../lib/scene/geometry";
  import { SIGN_WORDS, registerHead, registerMore } from "../../lib/scene/text";

  interface Props {
    rows: RegisterRow[];
  }
  let { rows }: Props = $props();

  let shown = $derived(rows.slice(0, REGISTER.rows));
  let hidden = $derived(rows.length - shown.length);
</script>

<div
  class="register"
  data-testid="register"
  aria-hidden="true"
  style:pointer-events="none"
  style:top="{REGISTER.top}px"
  style:right="{REGISTER.right}px"
  style:width="{REGISTER.w}px"
  style:--register-head={REGISTER.head}
  style:--register-row={REGISTER.rowH}
>
  <div class="head">
    <span class="count" data-testid="register-count">{registerHead(rows.length)}</span>
    <span class="order">{SIGN_WORDS.boardOrder}</span>
  </div>
  {#each shown as row (row.key)}
    {@const approval = row.number === null && row.approval}
    <div class="row" data-register-row={row.key}>
      <span class="ticket" class:alert={row.alert} class:approval data-approval={approval ? "" : undefined}>{row.number ?? ""}</span>
      <span class="name">{row.name}</span>
      {#if approval}<span class="tag">{SIGN_WORDS.approval}</span>{/if}
    </div>
  {/each}
  {#if hidden > 0}
    <div class="row more">{registerMore(hidden)}</div>
  {/if}
</div>

<style>
  .register {
    position: absolute;
    box-sizing: border-box;
    padding: 0 8px 6px 10px;
    border: 2px solid var(--color-attention-border);
    border-radius: var(--radius-surface);
    background: var(--color-surface);
    color: var(--color-text);
    font-weight: 700;
    font-size: 15px;
    line-height: 1;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    height: calc(var(--register-head) * 1px);
    line-height: calc(var(--register-head) * 1px);
    white-space: nowrap;
    overflow: hidden;
  }
  .count {
    font-size: 16px;
    font-weight: 800;
  }
  .order {
    color: var(--color-text-muted);
    font-size: 13px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: calc(var(--register-row) * 1px);
  }
  .ticket {
    flex: none;
    display: grid;
    place-items: center;
    width: 19px;
    height: 19px;
    border-radius: var(--radius-round);
    background: var(--color-attention);
    color: var(--color-attention-text);
    font-size: 12.5px;
    font-weight: 800;
  }
  .ticket.alert {
    background: var(--color-alert);
    color: var(--color-on-alert);
  }
  /* The approval-only wait's diamond, as on its figure (D-556). */
  .ticket.approval {
    clip-path: polygon(50% 0, 100% 50%, 50% 100%, 0 50%);
  }
  /* A line height of its own, so a clipped name keeps its descenders. */
  .name {
    min-width: 0;
    line-height: 1.3;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .tag {
    flex: none;
    margin-left: auto;
    color: var(--color-text-muted);
    font-size: 13px;
    font-weight: 600;
  }
  .more {
    font-size: 14px;
  }
</style>
