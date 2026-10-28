// The object layer (AD-59 layer 3, AD-171), owned here and not by Svelte
// templates: a keyed block would move figure nodes back where the depth
// sort took them from. It holds the statics of `furniture.ts`, sorted by
// depth `gx + gy`, and one persistent node per figure inserted among them
// by its own depth (its seat's when seated, ties by creation order). A
// figure is placed at its layout place at once (story 4.10 adds walking),
// at a position snapped to whole device pixels. Every attribute is written
// only when its cached value changes, so an unchanged layout writes
// nothing. Ported from the prototype v2's `renderActor`, `staticIndexFor`
// and `updateSnap`.
import type { ProjectColor } from "../generated/ProjectColor";
import { projector } from "./geometry";
import { ALERT_PAINTS, buildStatics, paint, type OfficeUse, type Static } from "./furniture";
import { alertLit, type Employee, type SceneLayout } from "./layout";
import { allSpots, restPoint, type Spot } from "./places";
import { badgeMarkup, createRig, lookOf, marksMarkup, setPose, ticketMarkup, underMarkup, write, type Rig } from "./rig";

const SVG_NS = "http://www.w3.org/2000/svg";

/** A figure stands just in front of a static at its very depth. */
const DEPTH_EPSILON = 0.001;
/** Where a kept desk's number floats above its empty chair, in px. */
const AWAY_LIFT = 25;
/** A free desk is drawn faded. */
const FREE_OPACITY = "0.42";

export interface ObjectsInput {
  layout: SceneLayout;
  /** Each live project's colour, by name: a figure's cap and collar. */
  colours: ReadonlyMap<string, ProjectColor | null>;
  /** Device pixels per logical px: the scene's scale times the device
   *  pixel ratio. */
  snap: number;
}

interface Figure {
  rig: Rig;
  colour: ProjectColor | null;
  /** Creation order, the depth sort's last tie-break. */
  token: number;
  depth: number;
}

interface DeskParts {
  desk: Element;
  chair: Element;
  screens: Element[];
  lines: Element[];
  away: Element;
  /** The chair's screen point, where a kept desk's number floats. */
  x: number;
  y: number;
  slot: number;
  cache: string | null;
}

interface StaticNode {
  node: Element;
  depth: number;
}

/** What each office slot of `layout` holds. */
export function officeUses(layout: SceneLayout): (OfficeUse | undefined)[] {
  const uses: (OfficeUse | undefined)[] = [];
  for (const room of layout.rooms) {
    if (room.slot === null) continue;
    uses[room.slot] = room.kind === "more" ? { kind: "more" } : { kind: "office", colour: room.project?.color ?? null };
  }
  return uses;
}

function staticsKey(columns: number, uses: readonly (OfficeUse | undefined)[]): string {
  const slots = Array.from(uses, (use) => (use === undefined ? "-" : use.kind === "more" ? "more" : (use.colour ?? "none")));
  return `${columns}|${slots.join(",")}`;
}

export class SceneObjects {
  readonly #root: SVGGElement;
  #statics: StaticNode[] = [];
  #staticsKey: string | null = null;
  #desks = new Map<string, DeskParts>();
  #deskColours: (ProjectColor | null)[] = [];
  #decisionTops: Element[] = [];
  #queueOn: boolean | null = null;
  /** The alert room's lit parts, by what `ALERT_PAINTS` paints them. */
  #alertParts: { part: keyof typeof ALERT_PAINTS; node: Element }[] = [];
  #beams: Element[] = [];
  #alertOn: boolean | null = null;
  #figures = new Map<string, Figure>();
  #created = 0;
  #spots = new Map<string, Spot>();
  #spotsColumns: number | null = null;

  constructor(root: SVGGElement) {
    this.#root = root;
  }

  /** The figure keys, in the layer's order: for tests and the e2e. */
  figureOrder(): string[] {
    return [...this.#root.querySelectorAll("[data-figure]")].map((el) => el.getAttribute("data-figure")!);
  }

  update({ layout, colours, snap }: ObjectsInput): void {
    const uses = officeUses(layout);
    const key = staticsKey(layout.columns, uses);
    if (key !== this.#staticsKey) {
      this.#rebuildStatics(buildStatics(layout.columns, uses), uses);
      this.#staticsKey = key;
    }
    if (this.#spotsColumns !== layout.columns) {
      this.#spots = new Map(allSpots(layout.columns).map((s) => [s.id, s]));
      this.#spotsColumns = layout.columns;
    }
    this.#applyDesks(layout);
    this.#applyQueue(layout);
    this.#applyAlert(layout);
    this.#applyFigures(layout, colours, snap);
    this.#reorder();
  }

  destroy(): void {
    this.#root.replaceChildren();
    this.#figures.clear();
    this.#desks.clear();
    this.#statics = [];
    this.#staticsKey = null;
  }

  #rebuildStatics(statics: Static[], uses: readonly (OfficeUse | undefined)[]): void {
    for (const { node } of this.#statics) node.remove();
    const holder = this.#root.ownerDocument.createElementNS(SVG_NS, "g");
    holder.innerHTML = statics
      .map((s) => {
        const desk = s.desk ? ` data-desk="${s.desk.slot}:${s.desk.index}" data-desk-part="${s.desk.part}"` : "";
        const partition = s.partition ? ` data-partition="${s.partition}"` : "";
        return `<g data-static data-depth="${s.depth}"${desk}${partition}>${s.markup}</g>`;
      })
      .join("");
    const nodes = [...holder.children];
    this.#statics = nodes.map((node, i) => ({ node, depth: statics[i]!.depth }));
    this.#root.append(...nodes);
    this.#desks.clear();
    this.#deskColours = Array.from(uses, (use) => (use?.kind === "office" ? use.colour : null));
    for (const node of nodes) {
      const id = node.getAttribute("data-desk");
      if (id === null) continue;
      const parts = this.#desks.get(id) ?? ({ cache: null } as DeskParts);
      if (node.getAttribute("data-desk-part") === "desk") {
        parts.desk = node;
        parts.screens = [...node.querySelectorAll("[data-screen]")];
        parts.lines = [...node.querySelectorAll("[data-lines]")];
      } else {
        parts.chair = node;
        parts.away = node.querySelector("[data-away]")!;
      }
      parts.slot = Number(id.split(":")[0]);
      this.#desks.set(id, parts);
    }
    this.#decisionTops = [...this.#root.querySelectorAll('[data-partition="decision"] [data-top]')];
    this.#queueOn = null;
    const parts = (part: keyof typeof ALERT_PAINTS, selector: string) => [...this.#root.querySelectorAll(selector)].map((node) => ({ part, node }));
    this.#alertParts = [
      ...parts("lamp", "[data-beacon-lamp]"),
      ...parts("panel", "[data-status-panel]"),
      ...parts("mark", "[data-status-mark]"),
      ...parts("partition", '[data-partition="alert"] [data-top]'),
    ];
    this.#beams = [...this.#root.querySelectorAll("[data-beacon-beam]")];
    this.#alertOn = null;
  }

  #applyDesks(layout: SceneLayout): void {
    const p = projector(layout.columns);
    const desks = new Map(layout.desks.map((d) => [`${d.slot}:${d.index}`, d]));
    const zones = new Map(layout.employees.map((e) => [e.key, e.zone]));
    const spots = this.#spots;
    for (const [id, parts] of this.#desks) {
      const desk = desks.get(id);
      const free = !desk || desk.state === "free";
      const lit = desk?.lit ?? false;
      let away = "";
      if (desk && desk.state === "kept" && desk.number !== null) {
        const zone = desk.key !== null ? zones.get(desk.key) : undefined;
        away = zone === "lounge" ? `badge:${desk.number}` : `ticket:${desk.number}:${zone === "alert"}`;
        const spot = spots.get(desk.spot);
        if (spot) {
          const at = p(spot.gx, spot.gy);
          parts.x = Math.round(at.x * 10) / 10;
          parts.y = Math.round(at.y * 10) / 10;
        }
      }
      const key = `${free}|${lit}|${away}`;
      if (parts.cache === key) continue;
      parts.cache = key;
      for (const el of [parts.desk, parts.chair]) {
        if (free) el.setAttribute("opacity", FREE_OPACITY);
        else el.removeAttribute("opacity");
      }
      for (const screen of parts.screens) screen.setAttribute("fill", paint(lit ? "color-working" : "scene-screen-off"));
      for (const lines of parts.lines) {
        if (lit) lines.removeAttribute("display");
        else lines.setAttribute("display", "none");
      }
      let mark = "";
      if (away !== "" && desk?.number != null) {
        const inner = away.startsWith("badge")
          ? badgeMarkup(desk.number, this.#deskColours[parts.slot] ?? null)
          : ticketMarkup(desk.number, away.endsWith("true"));
        mark = `<g transform="translate(${parts.x} ${parts.y - AWAY_LIFT})">${inner}</g>`;
      }
      parts.away.innerHTML = mark;
    }
  }

  /** The decision office's partitions turn amber while someone waits there. */
  #applyQueue(layout: SceneLayout): void {
    const on = layout.rooms.some((room) => room.kind === "decision" && room.plaque.count > 0);
    if (on === this.#queueOn) return;
    this.#queueOn = on;
    for (const top of this.#decisionTops) top.setAttribute("fill", paint(on ? "color-attention" : "scene-part-t"));
  }

  /** The alert room lights while someone stands in it (AD-175): the
   *  beacon's lamp and static beam, the status board, the partitions. */
  #applyAlert(layout: SceneLayout): void {
    const on = alertLit(layout);
    if (on === this.#alertOn) return;
    this.#alertOn = on;
    for (const { part, node } of this.#alertParts) node.setAttribute("fill", paint(ALERT_PAINTS[part][on ? "lit" : "quiet"]));
    for (const beam of this.#beams) {
      if (on) beam.removeAttribute("display");
      else beam.setAttribute("display", "none");
    }
  }

  #applyFigures(layout: SceneLayout, colours: ReadonlyMap<string, ProjectColor | null>, snap: number): void {
    const p = projector(layout.columns);
    const placed = layout.employees.filter((e): e is Employee & { spot: string; at: NonNullable<Employee["at"]> } => e.spot !== null && e.at !== null);
    const live = new Set(placed.map((e) => e.key));
    for (const [key, figure] of this.#figures) {
      if (live.has(key)) continue;
      figure.rig.node.remove();
      this.#figures.delete(key);
    }
    for (const e of placed) {
      const spot = this.#spots.get(e.spot);
      if (!spot) continue;
      const colour = colours.get(e.project) ?? null;
      let figure = this.#figures.get(e.key);
      if (figure && figure.colour !== colour) {
        figure.rig.node.remove();
        figure = undefined;
      }
      if (!figure) {
        const rig = createRig(this.#root.ownerDocument, { look: lookOf(e.key), colour, cap: true, scale: 1 });
        rig.node.setAttribute("data-figure", e.key);
        rig.node.setAttribute("aria-hidden", "true");
        figure = { rig, colour, token: this.#created++, depth: 0 };
        this.#figures.set(e.key, figure);
      }
      figure.depth = spot.gx + spot.gy + DEPTH_EPSILON;
      const { rig } = figure;
      const x = Math.round(e.at.x * snap) / snap;
      const y = Math.round(e.at.y * snap) / snap;
      write(rig, "transform", rig.node, "transform", `translate(${x} ${y})`);
      setPose(rig, e.pose, e.facing, e.marks.hand);
      // An unknown employee keeps its alert (D-494), and its ticket's colour.
      const marks = marksMarkup(e.marks, e.pose, e.facing, e.alert !== null, colour);
      if (rig.cache.get("marks") !== marks) {
        rig.cache.set("marks", marks);
        rig.marks.innerHTML = marks;
      }
      const floor = p(spot.gx, spot.gy);
      const rest = restPoint(spot);
      const restAt = p(rest.gx, rest.gy);
      const under = underMarkup(e.marks.ring, Math.round(floor.x - restAt.x), Math.round(floor.y - restAt.y));
      if (rig.cache.get("under") !== under) {
        rig.cache.set("under", under);
        rig.under.innerHTML = under;
      }
    }
  }

  /** The number of statics at or before `depth`. */
  #staticIndexFor(depth: number): number {
    let lo = 0;
    let hi = this.#statics.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (this.#statics[mid]!.depth <= depth) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  /** Inserts each figure before the first static deeper than it, or before
   *  the next figure sharing that static; moves a node only when its
   *  neighbour changed. */
  #reorder(): void {
    const list = [...this.#figures.values()].map((figure) => ({ figure, si: this.#staticIndexFor(figure.depth) }));
    list.sort((a, b) => a.si - b.si || a.figure.depth - b.figure.depth || a.figure.token - b.figure.token);
    for (let i = list.length - 1; i >= 0; i--) {
      const { figure, si } = list[i]!;
      const next = list[i + 1];
      const ref = next && next.si === si ? next.figure.rig.node : (this.#statics[si]?.node ?? null);
      const node = figure.rig.node;
      if (node.parentNode !== this.#root || node.nextSibling !== ref) this.#root.insertBefore(node, ref);
    }
  }
}
