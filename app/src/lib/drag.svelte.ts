// Dragging an ended card onto To resume, Done or the discard target (story
// 4.15, the owner's item 3, D-542), on pointer events: on Windows, Tauri's
// file drop handling can take HTML drag and drop over in WebView2, while
// pointer events behave the same there and in Playwright's Chromium. A press becomes a drag
// only past `DRAG_THRESHOLD_PX`, so a click still focuses the session;
// Escape cancels. The card menu does the same without a pointer.
import type { Card } from "./generated/Card";
import { actionFor, targetsFor, type SettleTarget } from "./settle";

/** How far a press travels before it is a drag, in CSS px. */
export const DRAG_THRESHOLD_PX = 6;

/** The attribute a drop target carries, its value a `SettleTarget`. */
export const DROP_ATTRIBUTE = "data-drop-target";

class DragState {
  /** The card being dragged, if any. */
  card = $state<Card | null>(null);
  /** The target under the pointer that takes the card, if any. */
  over = $state<SettleTarget | null>(null);
  /** How far the card has travelled from where it was pressed. */
  dx = $state(0);
  dy = $state(0);

  /** True while a drag is on and `target` would take its card. */
  takes(target: SettleTarget): boolean {
    return this.card !== null && actionFor(this.card, target) !== null;
  }
}

export const drag = new DragState();

/** The drop target under a point, what lies in the dragged card left out:
 *  the card follows the pointer, inside the column it came from. */
function targetAt(x: number, y: number, dragged: HTMLElement): SettleTarget | null {
  for (const element of document.elementsFromPoint(x, y)) {
    if (dragged.contains(element)) continue;
    const target = element.closest(`[${DROP_ATTRIBUTE}]`)?.getAttribute(DROP_ATTRIBUTE);
    if (target) return target as SettleTarget;
  }
  return null;
}

interface DraggableParams {
  card: Card;
  onDrop: (card: Card, target: SettleTarget) => void;
}

/** Makes an ended card draggable; a live card, or one with nowhere to go,
 *  is not. Controls marked `data-no-drag` inside it keep their own press. */
export function draggable(node: HTMLElement, initial: DraggableParams) {
  let params = initial;
  let pressed: { x: number; y: number; id: number } | null = null;
  let dragging = false;
  /** Set when a drag ends: the click the press would give is not one. */
  let swallowClick = false;

  function end(): void {
    pressed = null;
    dragging = false;
    drag.card = null;
    drag.over = null;
    drag.dx = 0;
    drag.dy = 0;
  }

  function down(event: PointerEvent): void {
    swallowClick = false;
    if (event.button !== 0 || !event.isPrimary || targetsFor(params.card).length === 0) return;
    if ((event.target as Element).closest("[data-no-drag]")) return;
    pressed = { x: event.clientX, y: event.clientY, id: event.pointerId };
  }

  function move(event: PointerEvent): void {
    if (!pressed || event.pointerId !== pressed.id) return;
    const dx = event.clientX - pressed.x;
    const dy = event.clientY - pressed.y;
    if (!dragging) {
      if (Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;
      dragging = true;
      node.setPointerCapture(event.pointerId);
      drag.card = params.card;
    }
    drag.dx = dx;
    drag.dy = dy;
    const target = targetAt(event.clientX, event.clientY, node);
    drag.over = target !== null && actionFor(params.card, target) !== null ? target : null;
  }

  function up(event: PointerEvent): void {
    if (!pressed || event.pointerId !== pressed.id) return;
    const target = dragging ? drag.over : null;
    swallowClick = dragging;
    end();
    if (target) params.onDrop(params.card, target);
  }

  function click(event: MouseEvent): void {
    if (!swallowClick) return;
    swallowClick = false;
    event.stopPropagation();
    event.preventDefault();
  }

  function key(event: KeyboardEvent): void {
    if (event.key !== "Escape" || !dragging) return;
    event.preventDefault();
    swallowClick = true;
    end();
  }

  node.addEventListener("pointerdown", down);
  node.addEventListener("pointermove", move);
  node.addEventListener("pointerup", up);
  node.addEventListener("pointercancel", end);
  node.addEventListener("click", click, true);
  window.addEventListener("keydown", key, true);
  return {
    update(next: DraggableParams) {
      params = next;
    },
    destroy() {
      if (dragging) end();
      node.removeEventListener("pointerdown", down);
      node.removeEventListener("pointermove", move);
      node.removeEventListener("pointerup", up);
      node.removeEventListener("pointercancel", end);
      node.removeEventListener("click", click, true);
      window.removeEventListener("keydown", key, true);
    },
  };
}
