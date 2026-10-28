<script lang="ts">
  // Layer 3 (AD-59, AD-171): mounted here, owned by `lib/scene/objects.ts`.
  // The action hands each new layout to the mover first (AD-172), then the
  // layout and the mover's figures to the module, which writes only what
  // changed; between layouts the mover's frames reach the module directly.
  import type { ProjectColor } from "../../lib/generated/ProjectColor";
  import type { SceneLayout } from "../../lib/scene/layout";
  import type { Mover } from "../../lib/scene/mover";
  import { SceneObjects } from "../../lib/scene/objects";

  interface Props {
    layout: SceneLayout;
    colours: ReadonlyMap<string, ProjectColor | null>;
    snap: number;
    mover: Mover;
    walking: boolean;
  }
  let { layout, colours, snap, mover, walking }: Props = $props();

  interface Input {
    layout: SceneLayout;
    colours: ReadonlyMap<string, ProjectColor | null>;
    snap: number;
    mover: Mover;
    walking: boolean;
  }

  function objects(node: SVGGElement, input: Input) {
    const layer = new SceneObjects(node);
    const apply = (next: Input) => {
      next.mover.update(next.layout, next.walking);
      layer.update({ layout: next.layout, colours: next.colours, snap: next.snap, figures: next.mover.figures() });
    };
    apply(input);
    const unsubscribe = input.mover.subscribe((figures) => layer.frame(figures));
    return {
      update: apply,
      destroy: () => {
        unsubscribe();
        layer.destroy();
      },
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
  <g use:objects={{ layout, colours, snap, mover, walking }}></g>
</svg>
