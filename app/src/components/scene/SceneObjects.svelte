<script lang="ts">
  // Layer 3 (AD-59, AD-171): mounted here, owned by `lib/scene/objects.ts`.
  // The action hands every new input to the module, which writes only what
  // changed.
  import type { ProjectColor } from "../../lib/generated/ProjectColor";
  import type { SceneLayout } from "../../lib/scene/layout";
  import { SceneObjects, type ObjectsInput } from "../../lib/scene/objects";

  interface Props {
    layout: SceneLayout;
    colours: ReadonlyMap<string, ProjectColor | null>;
    snap: number;
  }
  let { layout, colours, snap }: Props = $props();

  function objects(node: SVGGElement, input: ObjectsInput) {
    const layer = new SceneObjects(node);
    layer.update(input);
    return {
      update: (next: ObjectsInput) => layer.update(next),
      destroy: () => layer.destroy(),
    };
  }
</script>

<svg
  class="layer"
  aria-hidden="true"
  style:pointer-events="none"
  width={layout.canvas.w}
  height={layout.canvas.h}
  viewBox="0 0 {layout.canvas.w} {layout.canvas.h}"
  data-layer="objects"
>
  <g use:objects={{ layout, colours, snap }}></g>
</svg>
