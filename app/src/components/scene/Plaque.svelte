<script lang="ts">
  // A room's plaque (AD-61, D-106): HTML so a long name ends in an ellipsis
  // inside its cap. Names render as text only.
  import type { ProjectColor } from "../../lib/generated/ProjectColor";
  import { projectColor } from "../../lib/colors";
  import type { PlaqueState } from "../../lib/scene/layout";

  interface Props {
    text: string;
    state: PlaqueState;
    /** Shown after the name; `null` for a sign without a count. */
    count: number | null;
    /** An office's project colour, as a dot before the name. */
    dot?: ProjectColor | null;
    /** The room's group, "+n" at the plaque's left end, where its target
     *  sits (D-524); 0 without one. */
    more?: number;
    /** The "+n" at the right end instead (the decision office, D-557). */
    moreAtEnd?: boolean;
    /** A smaller pill, for the "high priority" sign. */
    small?: boolean;
  }
  let { text, state, count, dot, more = 0, moreAtEnd = false, small = false }: Props = $props();
</script>

<div class="plaque {state}" class:small class:grouped={more > 0} class:end={moreAtEnd}>
  {#if more > 0}<span class="more" data-testid="plaque-more">+{more}</span>{/if}
  <span class="pill">
    {#if dot !== undefined}<span class="dot" style:background={projectColor(dot)}></span>{/if}
    <span class="name">{text}</span>
    <!-- A head count, behind a person glyph, so a project's name and its
         count never read as one session's name, "officina 1" (D-1044). -->
    {#if count !== null}<span class="count" data-plaque-count
        ><svg viewBox="0 0 10 12" width="8" height="10" aria-hidden="true"
          ><circle cx="5" cy="3" r="2.6" /><path d="M0.4 12a4.6 4.6 0 0 1 9.2 0z" /></svg
        >{count}</span
      >{/if}
  </span>
</div>

<style>
  .plaque {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    width: 100%;
    height: 100%;
    font-size: 12px;
    line-height: 1;
  }
  /* With a group, the "+n" takes the left end and the name the rest. */
  .grouped {
    justify-content: flex-start;
  }
  .grouped.end {
    flex-direction: row-reverse;
  }
  .pill {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    height: 100%;
    padding: 0 10px;
    border-radius: var(--radius-control);
    background: var(--color-text);
    color: var(--color-bg);
    font-weight: 700;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .count {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 3px;
    padding-left: 6px;
    border-left: 1px solid currentColor;
    opacity: 0.72;
  }
  .count svg {
    fill: currentColor;
  }
  .dot {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: var(--radius-round);
    box-shadow: 0 0 0 1.6px var(--color-bg);
  }
  .quiet .pill,
  .dimmed .pill {
    background: var(--color-surface);
    color: var(--color-text);
    box-shadow: inset 0 0 0 1.2px var(--color-border);
  }
  .quiet .count,
  .dimmed .count {
    color: var(--color-text-muted);
    opacity: 1;
  }
  .quiet .dot,
  .dimmed .dot {
    box-shadow: 0 0 0 1.6px var(--color-surface);
  }
  .dimmed .name {
    color: var(--color-text-muted);
  }
  .attention .pill {
    background: var(--color-attention);
    color: var(--color-attention-text);
    box-shadow: inset 0 0 0 1.2px var(--color-attention-border);
  }
  .attention .count,
  .alert .count {
    opacity: 1;
  }
  .alert .pill {
    background: var(--color-alert);
    color: var(--color-on-alert);
  }
  .small {
    font-size: 10.5px;
  }
  .more {
    flex: none;
    display: flex;
    align-items: center;
    height: 100%;
    padding: 0 8px;
    border-radius: var(--radius-pill);
    background: var(--color-surface);
    color: var(--color-text);
    box-shadow: inset 0 0 0 1.2px var(--color-text);
    font-weight: 700;
  }
</style>
