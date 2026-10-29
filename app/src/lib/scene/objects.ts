// The object layer (AD-59 layer 3, AD-171), owned here and not by Svelte
// templates: a keyed block would move figure nodes back where the depth
// sort took them from. It holds the statics of `furniture.ts`, sorted by
// depth `gx + gy`, and one persistent node per figure inserted among them
// by its own depth (its seat's when seated, ties by creation order). A
// figure is drawn where the mover's frame says (`mover.ts`), at a position
// snapped to whole device pixels; a moving figure changes place among the
// statics with hysteresis, so walking along a desk never flickers in front
// of and behind it. Every attribute is written only when its cached value
// changes, so a settled scene receives no write. The depth order is split
// into consecutive bands (`geometry.ts` `DEPTH_BANDS`), one `<svg>` each,
// so a walk dirties one band's paint, never the whole layer's (story 4.16).
// Queue badges, a figure's and a kept desk's, are drawn in a layer of their
// own above every band, so no object ever covers one (story 4.17, D-554).
// Ported from the prototype v2's `renderActor`, `reorder`, `staticIndexFor` and
// `updateSnap`.
import type { ProjectColor } from "../generated/ProjectColor";
import { projector } from "./geometry";
import { ALERT_PAINTS, buildStatics, paint, type OfficeUse, type Static } from "./furniture";
import { SUBAGENT_SCALE, alertLit, type SceneLayout } from "./layout";
import type { FigureFrame } from "./mover";
import { allSpots, type Spot } from "./places";
import { createRig, deskTagMarkup, lookOf, marksMarkup, setPose, ticketMarkup, ticketOn, underMarkup, write, type Rig } from "./rig";

const SVG_NS = "http://www.w3.org/2000/svg";

/** A figure stands just in front of a static at its very depth. */
const DEPTH_EPSILON = 0.001;
/** A moving figure keeps its index among statics while its depth stays
 *  within this window of the statics around it, in tiles (AD-171). */
export const DEPTH_WINDOW = 0.05;
/** A moving figure's depth, as ties among figures read it, changes only
 *  past this step, in tiles (AD-171). */
export const DEPTH_QUANTUM = 0.06;
/** Where a kept desk's badge or tag floats above its empty chair, in px. */
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
  /** Every figure as the mover draws it now. */
  figures: ReadonlyMap<string, FigureFrame>;
}

interface Figure {
  rig: Rig;
  /** Its queue badge's node, in the badge layer. */
  badge: SVGGElement;
  colour: ProjectColor | null;
  /** Creation order, the depth sort's last tie-break. */
  token: number;
  depth: number;
  moving: boolean;
  /** Its index among statics and its quantised depth, held while it moves;
   *  `-1` before its first sort. */
  si: number;
  dq: number;
  /** The frame object last drawn: the mover hands the same one again
   *  while the figure rests, and it is not drawn twice. */
  drawn: FigureFrame | null;
}

interface DeskParts {
  desk: Element;
  chair: Element;
  screens: Element[];
  lines: Element[];
  /** Where a kept desk's tag floats, in the chair's static. */
  away: Element;
  /** Where a kept desk's queue badge floats, in the badge layer. */
  ticket: Element;
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
  /** One root per depth band, in paint order; each is its own paint
   *  layer, so a walk repaints only the band it walks in. */
  readonly #roots: SVGGElement[];
  /** The badge layer's groups: the kept desks' badges, then the figures'. */
  readonly #deskBadges: SVGGElement;
  readonly #figureBadges: SVGGElement;
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
  #columns = 0;
  #colours: ReadonlyMap<string, ProjectColor | null> = new Map();
  #snap = 1;

  constructor(roots: SVGGElement | readonly SVGGElement[], badges: SVGGElement) {
    this.#roots = Array.isArray(roots) ? [...(roots as readonly SVGGElement[])] : [roots as SVGGElement];
    this.#deskBadges = this.#doc.createElementNS(SVG_NS, "g");
    this.#figureBadges = this.#doc.createElementNS(SVG_NS, "g");
    badges.replaceChildren(this.#deskBadges, this.#figureBadges);
  }

  get #doc(): Document {
    return this.#roots[0]!.ownerDocument;
  }

  #all(selector: string): Element[] {
    return this.#roots.flatMap((root) => [...root.querySelectorAll(selector)]);
  }

  /** The figure keys, in the layer's order: for tests and the e2e. */
  figureOrder(): string[] {
    return this.#all("[data-figure]").map((el) => el.getAttribute("data-figure")!);
  }

  /** One frame of the mover: the figures only, on the last layout. */
  frame(figures: ReadonlyMap<string, FigureFrame>): void {
    this.#applyFigures(figures);
    this.#reorder();
  }

  update({ layout, colours, snap, figures }: ObjectsInput): void {
    this.#columns = layout.columns;
    this.#colours = colours;
    this.#snap = snap;
    // A new layout, scale or colour can change how an unchanged frame draws.
    for (const figure of this.#figures.values()) figure.drawn = null;
    const uses = officeUses(layout);
    const key = staticsKey(layout.columns, uses);
    if (key !== this.#staticsKey) {
      this.#rebuildStatics(buildStatics(layout.columns, uses), uses);
      this.#staticsKey = key;
      for (const figure of this.#figures.values()) figure.si = -1;
    }
    if (this.#spotsColumns !== layout.columns) {
      this.#spots = new Map(allSpots(layout.columns).map((s) => [s.id, s]));
      this.#spotsColumns = layout.columns;
    }
    this.#applyDesks(layout);
    this.#applyQueue(layout);
    this.#applyAlert(layout);
    this.#applyFigures(figures);
    this.#reorder();
  }

  destroy(): void {
    for (const root of this.#roots) root.replaceChildren();
    this.#deskBadges.replaceChildren();
    this.#figureBadges.replaceChildren();
    this.#figures.clear();
    this.#desks.clear();
    this.#statics = [];
    this.#staticsKey = null;
  }

  #rebuildStatics(statics: Static[], uses: readonly (OfficeUse | undefined)[]): void {
    for (const { node } of this.#statics) node.remove();
    const holder = this.#doc.createElementNS(SVG_NS, "g");
    holder.innerHTML = statics
      .map((s) => {
        const desk = s.desk ? ` data-desk="${s.desk.slot}:${s.desk.index}" data-desk-part="${s.desk.part}"` : "";
        const partition = s.partition ? ` data-partition="${s.partition}"` : "";
        return `<g data-static data-depth="${s.depth}"${desk}${partition}>${s.markup}</g>`;
      })
      .join("");
    const nodes = [...holder.children];
    this.#statics = nodes.map((node, i) => ({ node, depth: statics[i]!.depth }));
    const bands = this.#roots.length;
    nodes.forEach((node, i) => this.#roots[Math.floor((i * bands) / nodes.length)]!.append(node));
    this.#desks.clear();
    this.#deskBadges.replaceChildren();
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
        parts.ticket = this.#doc.createElementNS(SVG_NS, "g");
        parts.ticket.setAttribute("data-away-ticket", id);
        this.#deskBadges.append(parts.ticket);
      }
      parts.slot = Number(id.split(":")[0]);
      this.#desks.set(id, parts);
    }
    this.#decisionTops = this.#all('[data-partition="decision"] [data-top]');
    this.#queueOn = null;
    const parts = (part: keyof typeof ALERT_PAINTS, selector: string) => this.#all(selector).map((node) => ({ part, node }));
    this.#alertParts = [
      ...parts("lamp", "[data-beacon-lamp]"),
      ...parts("panel", "[data-status-panel]"),
      ...parts("mark", "[data-status-mark]"),
      ...parts("partition", '[data-partition="alert"] [data-top]'),
    ];
    this.#beams = this.#all("[data-beacon-beam]");
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
      let tag = "";
      let ticket = "";
      if (away !== "" && desk?.number != null) {
        const at = (inner: string) => `<g transform="translate(${parts.x} ${parts.y - AWAY_LIFT})">${inner}</g>`;
        if (away.startsWith("badge")) tag = at(deskTagMarkup(desk.number, this.#deskColours[parts.slot] ?? null));
        else ticket = at(ticketMarkup(desk.number, away.endsWith("true")));
      }
      parts.away.innerHTML = tag;
      parts.ticket.innerHTML = ticket;
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

  #applyFigures(figures: ReadonlyMap<string, FigureFrame>): void {
    const p = projector(this.#columns);
    const snap = this.#snap;
    for (const [key, figure] of this.#figures) {
      if (figures.has(key)) continue;
      figure.rig.node.remove();
      figure.badge.remove();
      this.#figures.delete(key);
    }
    for (const f of figures.values()) {
      let figure = this.#figures.get(f.key);
      // A figure walking out keeps its colour once its project has left.
      const colour = this.#colours.has(f.project) ? (this.#colours.get(f.project) ?? null) : (figure?.colour ?? null);
      if (figure && figure.colour !== colour) {
        figure.rig.node.remove();
        figure.badge.remove();
        figure = undefined;
      }
      if (!figure) {
        const rig = createRig(this.#doc, { look: lookOf(f.key), colour, cap: !f.helper, scale: f.helper ? SUBAGENT_SCALE : 1 });
        rig.node.setAttribute("data-figure", f.key);
        if (f.helper) rig.node.setAttribute("data-helper", "");
        rig.node.setAttribute("aria-hidden", "true");
        const badge = this.#doc.createElementNS(SVG_NS, "g");
        badge.setAttribute("data-ticket-of", f.key);
        this.#figureBadges.append(badge);
        figure = { rig, badge, colour, token: this.#created++, depth: 0, moving: false, si: -1, dq: 0, drawn: null };
        this.#figures.set(f.key, figure);
      }
      if (figure.drawn === f) continue;
      figure.drawn = f;
      figure.depth = f.gx + f.gy + DEPTH_EPSILON;
      figure.moving = f.moving;
      const { rig } = figure;
      const at = p(f.drawGx, f.drawGy);
      const x = Math.round(at.x * snap) / snap;
      const y = Math.round(at.y * snap) / snap;
      write(rig, "transform", rig.node, "transform", `translate(${x} ${y})`);
      write(rig, "opacity", rig.node, "opacity", f.opacity < 1 ? String(f.opacity) : null);
      setPose(rig, f.pose, f.facing, f.marks.hand, { flip: f.flip, stride: f.stride });
      const marks = marksMarkup(f.marks, f.pose, f.facing, colour);
      if (rig.cache.get("marks") !== marks) {
        rig.cache.set("marks", marks);
        rig.marks.innerHTML = marks;
      }
      // The badge follows its figure only while it shows, so a walk, which
      // shows none (D-531), writes nothing in the badge layer. An unknown
      // employee keeps its alert (D-494), and its badge's colour.
      const ticket = ticketOn(f.marks, f.pose, f.facing, f.alerted);
      if (rig.cache.get("ticket") !== ticket) {
        rig.cache.set("ticket", ticket);
        figure.badge.innerHTML = ticket;
      }
      if (ticket !== "") {
        write(rig, "ticket:transform", figure.badge, "transform", `translate(${x} ${y})`);
        write(rig, "ticket:opacity", figure.badge, "opacity", f.opacity < 1 ? String(f.opacity) : null);
      }
      const floor = p(f.gx, f.gy);
      const under = underMarkup(f.marks.ring, Math.round(floor.x - at.x), Math.round(floor.y - at.y));
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
   *  neighbour changed. A figure at rest takes its exact index; a moving
   *  one keeps its index while its depth stays within `DEPTH_WINDOW` of
   *  the statics around it, and its tie depth while it moves less than
   *  `DEPTH_QUANTUM` (the prototype v2's hysteresis). */
  #reorder(): void {
    for (const figure of this.#figures.values()) {
      const d = figure.depth;
      if (!figure.moving || figure.si < 0) {
        figure.si = this.#staticIndexFor(d);
        figure.dq = d;
        continue;
      }
      if (Math.abs(d - figure.dq) > DEPTH_QUANTUM) figure.dq = d;
      const lo = this.#staticIndexFor(d - DEPTH_WINDOW);
      const hi = this.#staticIndexFor(d + DEPTH_WINDOW);
      if (figure.si < lo || figure.si > hi) figure.si = this.#staticIndexFor(d);
    }
    const list = [...this.#figures.values()];
    list.sort((a, b) => a.si - b.si || a.dq - b.dq || a.token - b.token);
    for (let i = list.length - 1; i >= 0; i--) {
      const figure = list[i]!;
      const next = list[i + 1];
      const ref = next && next.si === figure.si ? next.rig.node : (this.#statics[figure.si]?.node ?? null);
      const node = figure.rig.node;
      const parent = ref?.parentNode ?? this.#roots[this.#roots.length - 1]!;
      if (node.parentNode !== parent || node.nextSibling !== ref) parent.insertBefore(node, ref);
    }
  }
}
