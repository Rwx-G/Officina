<script lang="ts">
  // The workshop's images (AD-99, AD-118, AD-121): each one's state, the
  // label its comparison stopped at, and the projects whose inventory it
  // holds. Build images starts containers, so it acts on a pointer click
  // only (D-28).
  import type { ImageView } from "../lib/generated/ImageView";
  import { isPointerClick } from "../lib/pointer";
  import { ago } from "../lib/time";
  import { workshop } from "../lib/workshop.svelte";
  import { BUILD_REFUSAL_TEXT, IMAGE_KIND_TEXT, IMAGE_MISMATCH_TEXT, IMAGE_STATE_TEXT } from "../lib/workshop-text";

  interface Props {
    images: ImageView[];
    nowMs: number;
  }
  let { images, nowMs }: Props = $props();

  let building = $derived(workshop.building || images.some((image) => image.state === "building"));

  function build(event: MouseEvent): void {
    if (!isPointerClick(event)) return;
    void workshop.buildImages();
  }

  function noKeyStarts(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") event.preventDefault();
  }
</script>

<div class="part" data-testid="image-table">
  <div class="head">
    <h3>Images</h3>
    <button type="button" data-testid="build-images" disabled={building} onclick={build} onkeydown={noKeyStarts}>
      Build images
    </button>
  </div>
  {#if workshop.build?.outcome === "queued"}
    <p class="muted" data-testid="build-queued">Queued: each image's state follows its build.</p>
  {:else if workshop.build?.outcome === "refused"}
    <p class="fault" data-testid="build-refused" data-refusal={workshop.build.detail}>
      Refused: {BUILD_REFUSAL_TEXT[workshop.build.detail]}
    </p>
  {:else if workshop.buildError}
    <p class="fault">{workshop.buildError}</p>
  {/if}
  {#if images.length === 0}
    <p class="muted" data-testid="images-none">No image listed: the Linux programs or Docker are not found.</p>
  {:else}
    <table>
      <thead>
        <tr><th>Image</th><th>Kind</th><th>State</th><th>Inventories</th></tr>
      </thead>
      <tbody>
        {#each images as image (image.tag)}
          <tr data-testid="image-row" data-tag={image.tag} data-state={image.state}>
            <td><code>{image.tag}</code></td>
            <td>{IMAGE_KIND_TEXT[image.kind]}</td>
            <td data-testid="image-state">
              {IMAGE_STATE_TEXT[image.state]}{#if image.mismatch}<span class="muted">
                  : {IMAGE_MISMATCH_TEXT[image.mismatch]}{#if image.label}, at <code>{image.label}</code>{/if}</span
                >{/if}
            </td>
            <td data-testid="image-inventories">
              {#if image.inventories.length === 0}
                <span class="muted">none</span>
              {:else}
                {#each image.inventories as inventory, i (inventory.project)}
                  {i > 0 ? ", " : ""}{inventory.project} <span class="muted">{ago(inventory.at_ms, nowMs)}</span>
                {/each}
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
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
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th,
  td {
    padding: 4px 6px;
    text-align: left;
    vertical-align: top;
    border-top: 1px solid var(--color-border);
    overflow-wrap: anywhere;
  }
  th {
    font-weight: 600;
    color: var(--color-text-muted);
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
  button:disabled {
    cursor: default;
    opacity: 0.6;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12.5px;
  }
  .muted {
    color: var(--color-text-muted);
  }
  .fault {
    color: var(--color-stopped-text);
    font-weight: 600;
  }
</style>
