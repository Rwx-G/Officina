<script lang="ts">
  // Each image's newest escape battery (AD-117): its outcome, its age and
  // whether it keeps the image valid on the engine as read now. A battery
  // that no longer does is named stale. Run escape battery runs every
  // stale image's battery; it starts containers, so it acts on a pointer
  // click only (D-28).
  import type { ImageView } from "../lib/generated/ImageView";
  import { isPointerClick } from "../lib/pointer";
  import { ago } from "../lib/time";
  import { workshop } from "../lib/workshop.svelte";
  import { BATTERY_OUTCOME_TEXT, BATTERY_REFUSAL_TEXT } from "../lib/workshop-text";

  interface Props {
    images: ImageView[];
    nowMs: number;
  }
  let { images, nowMs }: Props = $props();

  let tested = $derived(images.filter((image) => image.battery !== null));

  function validity(valid: boolean | null): "valid" | "stale" | "unknown" {
    return valid === null ? "unknown" : valid ? "valid" : "stale";
  }

  const VALIDITY_TEXT = {
    valid: "valid",
    stale: "stale: run the battery again before a launch",
    unknown: "unknown until Docker answers",
  } as const;

  function run(event: MouseEvent): void {
    if (!isPointerClick(event)) return;
    void workshop.runBattery(null);
  }

  function noKeyStarts(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") event.preventDefault();
  }
</script>

<div class="part" data-testid="battery-status">
  <div class="head">
    <h3>Escape battery</h3>
    <button type="button" data-testid="run-battery" onclick={run} onkeydown={noKeyStarts}>Run escape battery</button>
  </div>
  {#if workshop.battery?.outcome === "queued"}
    <p class="muted" data-testid="battery-queued">Queued: {workshop.battery.detail.join(", ")}.</p>
  {:else if workshop.battery?.outcome === "refused"}
    <p class="fault" data-testid="battery-refused" data-refusal={workshop.battery.detail}>
      Refused: {BATTERY_REFUSAL_TEXT[workshop.battery.detail]}
    </p>
  {:else if workshop.batteryError}
    <p class="fault">{workshop.batteryError}</p>
  {/if}
  {#if tested.length === 0}
    <p class="muted" data-testid="battery-none">No image to test.</p>
  {:else}
    <ul>
      {#each tested as image (image.tag)}
        {@const battery = image.battery!}
        {@const state = validity(battery.valid)}
        <li data-testid="battery-row" data-tag={image.tag} data-validity={state}>
          <code>{image.tag}</code>
          {#if battery.outcome === null}
            <span>never run</span>
          {:else}
            <span data-testid="battery-outcome">{BATTERY_OUTCOME_TEXT[battery.outcome]}</span>
            {#if battery.finished_at_ms !== null}<span class="muted">{ago(battery.finished_at_ms, nowMs)}</span>{/if}
          {/if}
          <span class:flag={state === "stale"} data-testid="battery-validity">{VALIDITY_TEXT[state]}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  h3 {
    margin: 8px 0 4px;
    font-size: 14px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    padding: 4px 0;
    border-top: 1px solid var(--color-border);
    font-size: 13px;
  }
  p {
    margin: 4px 0;
    font-size: 13px;
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
  .muted {
    color: var(--color-text-muted);
  }
  .flag,
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
