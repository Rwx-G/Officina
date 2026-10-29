// A figure (AD-171, AD-173), ported from the prototype v2's "figure rig":
// one node built once per card key, holding three pose groups (standing,
// with a front and a back view under a flip group; seated facing the
// author; seated working, back turned, the chair back over the body). A
// pose change shows one group and hides the others; it never rebuilds
// markup, and every attribute goes through the rig's cache, so a figure
// whose pose is unchanged receives no write. The markup takes closed types
// only: a look, a project colour, the cap flag and the scale, never text.
import type { ProjectColor } from "../generated/ProjectColor";
import { CAPACITY, type Facing } from "./places";
import type { Marks, Pose } from "./layout";
import type { Stride } from "./mover";
import { SHADE_1, SHADE_2, fnv1a, paint, projectPaint } from "./furniture";

export const HAIR_STYLES = ["short", "bob", "bun", "curly", "crop"] as const;
export type HairStyle = (typeof HAIR_STYLES)[number];

/** The (skin, hair) pairs of app.css's `--skin-*` and `--hair-*` tokens
 *  with hair against skin at 3:1 or more, the back-of-head cue (D-478).
 *  tokens.test.ts recomputes them from app.css and fails when this list
 *  and the tokens disagree. */
export const LOOKS: readonly (readonly [skin: number, hair: number])[] = [
  [0, 0],
  [0, 1],
  [0, 2],
  [1, 0],
  [1, 1],
  [1, 2],
  [2, 0],
  [2, 1],
  [2, 2],
  [3, 0],
  [3, 1],
  [3, 2],
  [4, 0],
];

export interface Look {
  skin: number;
  hair: number;
  style: HairStyle;
}

/** A session's look for its life, from the FNV-1a hash of its card key; it
 *  carries no meaning (AD-173). */
export function lookOf(key: string): Look {
  const h = fnv1a(key);
  const [skin, hair] = LOOKS[h % LOOKS.length]!;
  return { skin, hair, style: HAIR_STYLES[(h >>> 8) % HAIR_STYLES.length]! };
}

export interface RigOptions {
  look: Look;
  /** The project's colour: collar, cap and working chair. */
  colour: ProjectColor | null;
  /** Every session's figure wears a cap (D-477); a sub-agent's does not. */
  cap: boolean;
  /** 1 for a session, smaller for a sub-agent (AD-176). */
  scale: number;
}

const r1 = (n: number): number => Math.round(n * 10) / 10;

const skinOf = (look: Look) => `var(--skin-${look.skin})`;
const hairOf = (look: Look) => `var(--hair-${look.hair})`;
const RIM = ` stroke="${paint("figure-rim")}" stroke-width="1.1" paint-order="stroke"`;

function hairFront(style: HairStyle, cy: number, H: string): string {
  if (style === "bob") return `<path ${H} d="M-9 ${cy + 6.5} L-9 ${cy} A9 9 0 0 1 9 ${cy} L9 ${cy + 6.5} L6.4 ${cy + 6.5} L6.6 ${cy - 1} Q1 ${cy - 5.5} -6.6 ${cy - 1} L-6.4 ${cy + 6.5} Z"/>`;
  if (style === "crop") return `<path ${H} d="M-8.3 ${cy - 0.5} A8.3 8.3 0 0 1 8.3 ${cy - 0.5} Q0 ${cy - 5} -8.3 ${cy - 0.5} Z"/>`;
  if (style === "curly") {
    let s = `<path ${H} d="M-8.4 ${cy} A8.4 8.4 0 0 1 8.4 ${cy} Q0 ${cy - 4.5} -8.4 ${cy} Z"/>`;
    for (const a of [-175, -145, -115, -85, -55, -25, 5]) {
      s += `<circle ${H} cx="${r1(8 * Math.cos((a * Math.PI) / 180))}" cy="${r1(cy + 7.6 * Math.sin((a * Math.PI) / 180))}" r="3.1"/>`;
    }
    return s;
  }
  if (style === "bun") return `<path ${H} d="M-8.4 ${cy + 1} A8.4 8.4 0 0 1 8.4 ${cy + 1} Q3 ${cy - 4.2} -1.5 ${cy - 3.4} Q-5 ${cy - 2.6} -8.4 ${cy + 1} Z"/><circle ${H} cx="0" cy="${cy - 9.4}" r="3.8"/>`;
  return `<path ${H} d="M-8.5 ${cy + 1.5} A8.5 8.5 0 0 1 8.5 ${cy + 1.5} Q4 ${cy - 4} -1 ${cy - 3} Q-5 ${cy - 2.2} -8.5 ${cy + 1.5} Z"/>`;
}

function hairBack(style: HairStyle, cy: number, H: string): string {
  let s =
    style === "bob"
      ? `<path ${H} d="M-9 ${cy + 7} L-9 ${cy} A9 9 0 0 1 9 ${cy} L9 ${cy + 7} Q0 ${cy + 8.6} -9 ${cy + 7} Z"/>`
      : `<path ${H} d="M-8.6 ${cy + 3.6} A8.6 8.6 0 1 1 8.6 ${cy + 3.6} Q0 ${cy + 6.2} -8.6 ${cy + 3.6} Z"/>`;
  if (style === "bun") s += `<circle ${H} cx="0" cy="${cy - 6}" r="4.2"/>`;
  if (style === "curly") {
    for (const a of [-180, -140, -100, -60, -20, 20, 160]) {
      s += `<circle ${H} cx="${r1(8.2 * Math.cos((a * Math.PI) / 180))}" cy="${r1(cy + 7.6 * Math.sin((a * Math.PI) / 180))}" r="3.1"/>`;
    }
  }
  return s;
}

/** A head centred on (0, cy): the face and hair from the front, the hair
 *  over the head from the back, and the cap. */
function head(cy: number, look: Look, cap: string | null, view: Facing): string {
  const S = `fill="${skinOf(look)}"`;
  const H = `fill="${hairOf(look)}"`;
  let s = `<g${RIM}><circle ${S} cx="0" cy="${cy}" r="8.2"/>`;
  if (view === "front") {
    s += `<path fill="${SHADE_1}" d="M2.5 ${cy + 8} A8.2 8.2 0 0 0 8.2 ${cy} L5.6 ${cy + 1} Q5 ${cy + 6} 2.5 ${cy + 8} Z"/>`;
    s += `<circle ${S} cx="-7.6" cy="${cy + 1.2}" r="1.8"/>`;
    s += hairFront(cap && look.style === "bun" ? "crop" : look.style, cy, H);
    s += `<circle fill="${paint("figure-eye")}" cx="-1.4" cy="${cy + 0.9}" r="1.15"/><circle fill="${paint("figure-eye")}" cx="3.8" cy="${cy + 0.9}" r="1.15"/>`;
    s += `<path stroke="${paint("figure-eye")}" d="M-0.2 ${cy + 4.4} Q1.4 ${cy + 5.5} 3 ${cy + 4.4}" fill="none" stroke-width="0.9" stroke-linecap="round" opacity="0.7"/>`;
  } else {
    s += hairBack(look.style, cy, H);
  }
  if (cap) {
    s += `<path fill="${cap}" d="M-8.9 ${cy - 1.2} A8.9 8.9 0 0 1 8.9 ${cy - 1.2} Z"/><path fill="${SHADE_1}" d="M2 ${cy - 9.6} A8.9 8.9 0 0 1 8.9 ${cy - 1.2} L2 ${cy - 1.2} Z"/>`;
    if (view === "front") {
      s += `<path fill="${cap}" d="M-2 ${cy - 1.8} L12.6 ${cy - 1.8} Q13.4 ${cy + 0.8} 9.6 ${cy + 0.6} L-2 ${cy + 0.2} Z"/><path fill="${SHADE_2}" d="M-2 ${cy - 0.4} L12.9 ${cy - 0.4} Q13.4 ${cy + 0.8} 9.6 ${cy + 0.6} L-2 ${cy + 0.2} Z"/>`;
    } else {
      s += `<rect fill="${SHADE_2}" x="-8.9" y="${cy - 3}" width="17.8" height="1.8" rx="0.9"/>`;
    }
    s += `<circle fill="${paint("figure-shoe")}" cx="0" cy="${cy - 10.1}" r="1.3"/>`;
  }
  return s + "</g>";
}

function torso(top: number, bottom: number, collar: string, view: Facing): string {
  const T = paint("figure-tunic");
  const TS = paint("figure-tunic-shade");
  let s = `<g${RIM}><path fill="${T}" d="M-7.6 ${bottom} L-9.8 ${top + 5.5} Q-10.6 ${top} -5 ${top - 0.4} L5 ${top - 0.4} Q10.6 ${top} 9.8 ${top + 5.5} L7.6 ${bottom} Z"/>`;
  s += `<path fill="${TS}" d="M1.5 ${bottom} L1.5 ${top - 0.4} L5 ${top - 0.4} Q10.6 ${top} 9.8 ${top + 5.5} L7.6 ${bottom} Z"/>`;
  s += `<rect fill="${paint("figure-legs")}" x="-7.9" y="${bottom - 2.4}" width="15.8" height="2.8" rx="1.2"/>`;
  if (view === "front") s += `<path fill="${collar}" d="M-5.2 ${top - 0.2} L0 ${top + 5} L5.2 ${top - 0.2} L3.2 ${top - 0.6} L0 ${top + 2.6} L-3.2 ${top - 0.6} Z"/>`;
  else s += `<path fill="${collar}" d="M-5.6 ${top - 0.4} Q0 ${top + 2} 5.6 ${top - 0.4} L5.6 ${top + 1.2} Q0 ${top + 3.6} -5.6 ${top + 1.2} Z"/>`;
  return s + "</g>";
}

/** An arm hanging from the shoulder at (x, top + 1.5), rotated as a group. */
function arm(part: string, x: number, top: number, look: Look, sleeve: string): string {
  return `<g data-part="${part}"><rect fill="${sleeve}" x="${r1(x - 2.3)}" y="${top + 0.5}" width="4.6" height="14" rx="2.3"/><circle fill="${skinOf(look)}" cx="${x}" cy="${top + 16}" r="2.6"/></g>`;
}

function leg(part: string, x: number, hip: number, shade: boolean): string {
  const rect = (fill: string) => `<rect fill="${fill}" x="${r1(x - 2.4)}" y="${hip}" width="4.8" height="${r1(-hip - 1.8)}" rx="2.2"/>`;
  return `<g data-part="${part}">${rect(paint("figure-legs"))}${shade ? rect(SHADE_1) : ""}<ellipse fill="${paint("figure-shoe")}" cx="${r1(x + 1.2)}" cy="-1.7" rx="3.7" ry="2"/></g>`;
}

/** The figure's markup, origin at its floor point. */
export function rigMarkup({ look, colour, cap, scale }: RigOptions): string {
  const tone = projectPaint(colour);
  const capFill = cap ? tone : null;
  const T = paint("figure-tunic");
  const TS = paint("figure-tunic-shade");
  const neck = (y: number) => `<rect fill="${skinOf(look)}" x="-2.6" y="${y}" width="5.2" height="5.5"/>`;
  let s = `<g data-part="under"></g><g data-part="rig"${scale !== 1 ? ` transform="scale(${scale})"` : ""}>`;
  s += `<g data-part="stand"><g data-part="flip"><g data-part="bob">`;
  s += leg("leg-far", -3.2, -21, true) + leg("leg-near", 3.2, -21, false);
  s += arm("arm-far", -10.6, -40, look, TS);
  s += `<g data-view="front">${torso(-40.5, -19.5, tone, "front")}</g><g data-view="back" display="none">${torso(-40.5, -19.5, tone, "back")}</g>`;
  s += neck(-45.5) + `<rect fill="${SHADE_1}" x="-2.6" y="-45.5" width="5.2" height="2.4"/>`;
  s += `<g data-view="front">${head(-52.5, look, capFill, "front")}</g><g data-view="back" display="none">${head(-52.5, look, capFill, "back")}</g>`;
  s += arm("arm-near", 10.6, -40, look, T);
  s += `</g></g></g>`;
  s += `<g data-part="seated-facing" display="none">`;
  s += `<rect fill="${paint("figure-legs")}" x="-6.1" y="-10" width="4.6" height="13" rx="2"/><rect fill="${paint("figure-legs")}" x="1.5" y="-10" width="4.6" height="13" rx="2"/><rect fill="${SHADE_1}" x="-6.1" y="-10" width="4.6" height="13" rx="2"/>`;
  s += `<ellipse fill="${paint("figure-shoe")}" cx="-3.4" cy="3.4" rx="3.6" ry="2"/><ellipse fill="${paint("figure-shoe")}" cx="4.2" cy="3.4" rx="3.6" ry="2"/>`;
  s += `<rect fill="${paint("figure-legs")}" x="-7" y="-16" width="6" height="7.5" rx="3"/><rect fill="${paint("figure-legs")}" x="1" y="-16" width="6" height="7.5" rx="3"/>`;
  s += arm("seated-arm-far", -10.6, -36, look, TS);
  s += torso(-36, -13.5, tone, "front");
  s += neck(-41);
  s += head(-48, look, capFill, "front");
  s += arm("seated-arm-near", 10.6, -36, look, T);
  s += `</g>`;
  s += `<g data-part="seated-working" display="none">`;
  s += `<ellipse fill="${TS}" cx="-11.6" cy="-24" rx="3.2" ry="5"/><ellipse fill="${TS}" cx="11.6" cy="-24" rx="3.2" ry="5"/>`;
  s += torso(-37.5, -16.5, tone, "back");
  s += neck(-42.5);
  s += head(-49.5, look, capFill, "back");
  s += `<rect x="-11.5" y="-32" width="23" height="20" rx="7" fill="${tone}"/><rect fill="${SHADE_1}" x="-11.5" y="-32" width="23" height="20" rx="7"/><rect fill="${SHADE_2}" x="-11.5" y="-16" width="23" height="4" rx="2"/>`;
  s += `</g></g><g data-part="marks"></g>`;
  return s;
}

const POSE_PART: Record<Pose, string> = {
  standing: "stand",
  "seated-facing": "seated-facing",
  "seated-working": "seated-working",
};

export interface Rig {
  node: SVGGElement;
  under: SVGGElement;
  marks: SVGGElement;
  poses: Record<Pose, SVGGElement>;
  fronts: SVGGElement[];
  backs: SVGGElement[];
  flip: SVGGElement;
  bob: SVGGElement;
  legFar: SVGGElement;
  legNear: SVGGElement;
  armFar: SVGGElement;
  armNear: SVGGElement;
  seatedArmFar: SVGGElement;
  seatedArmNear: SVGGElement;
  /** The last value written per attribute: a write happens on change only. */
  cache: Map<string, string>;
}

const SVG_NS = "http://www.w3.org/2000/svg";

/** A figure's node, built once, every pose group hidden but standing. */
export function createRig(doc: Document, options: RigOptions): Rig {
  const node = doc.createElementNS(SVG_NS, "g");
  node.innerHTML = rigMarkup(options);
  const part = (name: string) => node.querySelector<SVGGElement>(`[data-part="${name}"]`)!;
  return {
    node,
    under: part("under"),
    marks: part("marks"),
    poses: { standing: part("stand"), "seated-facing": part("seated-facing"), "seated-working": part("seated-working") },
    fronts: [...part("stand").querySelectorAll<SVGGElement>('[data-view="front"]')],
    backs: [...part("stand").querySelectorAll<SVGGElement>('[data-view="back"]')],
    flip: part("flip"),
    bob: part("bob"),
    legFar: part("leg-far"),
    legNear: part("leg-near"),
    armFar: part("arm-far"),
    armNear: part("arm-near"),
    seatedArmFar: part("seated-arm-far"),
    seatedArmNear: part("seated-arm-near"),
    cache: new Map(),
  };
}

/** Writes `value` to `name` on `element` unless the cache holds it; `null`
 *  removes the attribute. */
export function write(rig: Rig, key: string, element: Element, name: string, value: string | null): void {
  const cached = rig.cache.get(key) ?? null;
  if (rig.cache.has(key) && cached === value) return;
  rig.cache.set(key, value as string);
  if (value === null) element.removeAttribute(name);
  else element.setAttribute(name, value);
}

const shown = (on: boolean): string | null => (on ? null : "none");

/** How a standing figure is turned and moving: flipped to face left, and
 *  its stride while it walks (`null` at rest). */
export interface Motion {
  flip: boolean;
  stride: Stride | null;
}

const AT_REST: Motion = { flip: false, stride: null };
const rotate = (angle: number, pivot: string): string | null => (angle === 0 ? null : `rotate(${angle} ${pivot})`);

/** Shows `pose` and its view, its arms as the layout says, and the walk
 *  cycle the mover drives from script, never from CSS (AD-171). A limb at
 *  rest carries no transform. */
export function setPose(rig: Rig, pose: Pose, facing: Facing, hand: boolean, motion: Motion = AT_REST): void {
  for (const [name, group] of Object.entries(rig.poses) as [Pose, SVGGElement][]) {
    write(rig, `pose:${POSE_PART[name]}`, group, "display", shown(name === pose));
  }
  if (pose === "standing") {
    const front = facing === "front";
    rig.fronts.forEach((el, i) => write(rig, `front:${i}`, el, "display", shown(front)));
    rig.backs.forEach((el, i) => write(rig, `back:${i}`, el, "display", shown(!front)));
    const stride = motion.stride ?? { leg: 0, arm: 0, bob: 0 };
    write(rig, "flip", rig.flip, "transform", motion.flip ? "scale(-1 1)" : null);
    write(rig, "bob", rig.bob, "transform", stride.bob === 0 ? null : `translate(0 ${stride.bob})`);
    write(rig, "leg-far", rig.legFar, "transform", rotate(stride.leg, "-3.2 -21"));
    write(rig, "leg-near", rig.legNear, "transform", rotate(-stride.leg, "3.2 -21"));
    write(rig, "arm-far", rig.armFar, "transform", rotate(stride.arm, "-10.6 -38.5"));
    write(rig, "arm-near", rig.armNear, "transform", hand ? "rotate(-160 10.6 -38.5)" : rotate(-stride.arm, "10.6 -38.5"));
  } else if (pose === "seated-facing") {
    write(rig, "seated-arm-far", rig.seatedArmFar, "transform", "rotate(-16 -10.6 -34.5)");
    write(rig, "seated-arm-near", rig.seatedArmNear, "transform", hand ? "rotate(-160 10.6 -34.5)" : "rotate(16 10.6 -34.5)");
  }
}

// ------------------------------------------------------------ marks

/** A board number on amber, or on the alert colour in the alert room; an
 *  approval-only wait's ticket has no digit (D-464). */
export function ticketMarkup(number: number | null, alert: boolean): string {
  const [bg, fg] = alert ? [paint("color-alert"), paint("color-on-alert")] : [paint("color-attention"), paint("color-attention-text")];
  const circle = `<circle fill="${bg}" stroke="${paint("color-attention-text")}" cx="0" cy="0" r="7.6" stroke-width="1.3"/>`;
  return number === null ? circle : circle + glyph(String(integer(number)), fg);
}

/** The letter that names desk `desk` of an office, A for the first:
 *  desks are named by letters, never digits, so a digit on a badge is
 *  always a board key (story 4.17, D-558). */
export function deskLetter(desk: number): string {
  if (!Number.isInteger(desk) || desk < 1 || desk > CAPACITY.desks) throw new RangeError(`no desk ${desk} in an office`);
  return String.fromCharCode("A".charCodeAt(0) + desk - 1);
}

/** A desk's tag (D-558): its letter on a square in the surface colour,
 *  edged in the project's colour; never a disc, never a digit. */
export function deskTagMarkup(desk: number, colour: ProjectColor | null): string {
  return `<rect fill="${paint("color-surface")}" stroke="${projectPaint(colour)}" x="-7.4" y="-7.4" width="14.8" height="14.8" rx="2" stroke-width="2.2"/>${glyph(deskLetter(desk), paint("color-text"))}`;
}

function integer(number: number): number {
  if (!Number.isInteger(number)) throw new TypeError("a mark's number must be an integer");
  return number;
}

/** A number or a desk letter, never payload text, so no session text
 *  reaches markup. */
function glyph(text: string, fill: string): string {
  return `<text fill="${fill}" x="0" y="3.8" text-anchor="middle" font-size="10.5" font-weight="800">${text}</text>`;
}

/** The question mark of an employee whose state is unknown, drawn. */
function unknownMarkup(y: number): string {
  return (
    `<circle fill="${paint("color-surface")}" stroke="${paint("color-text-muted")}" cx="15" cy="${y}" r="8.5" stroke-width="1.5"/>` +
    `<path fill="none" stroke="${paint("color-text-muted")}" stroke-width="2.2" stroke-linecap="round" d="M11.8 ${r1(y - 2.6)} Q12 ${r1(y - 6)} 15 ${r1(y - 6)} Q18.2 ${r1(y - 6)} 18.2 ${r1(y - 3)} Q18.2 ${r1(y - 0.8)} 15 ${r1(y + 0.4)} L15 ${r1(y + 1.6)}"/>` +
    `<circle fill="${paint("color-text-muted")}" cx="15" cy="${r1(y + 4.6)}" r="1.3"/>`
  );
}

/** The height of a figure's chest above its floor point, or `null` when
 *  its chest is turned away from the author. */
function chestY(pose: Pose, facing: Facing): number | null {
  if (pose === "seated-working" || (pose === "standing" && facing === "back")) return null;
  return pose === "seated-facing" ? -26 : -31;
}

/** A figure's queue badge on its chest, origin at its floor point, or ""
 *  (D-554: drawn in the badge layer, above every object). */
export function ticketOn(marks: Marks, pose: Pose, facing: Facing, alert: boolean): string {
  const y = chestY(pose, facing);
  if (y === null || (marks.ticket === null && !marks.approval)) return "";
  return `<g data-mark="ticket" transform="translate(0 ${y})">${ticketMarkup(marks.ticket, alert)}</g>`;
}

/** The marks drawn with the figure: its desk's tag on the chest while the
 *  chest shows, and the question mark. */
export function marksMarkup(marks: Marks, pose: Pose, facing: Facing, colour: ProjectColor | null): string {
  const y = chestY(pose, facing);
  let s = "";
  if (y !== null && marks.deskBadge !== null) s += `<g data-mark="desk" transform="translate(0 ${y})">${deskTagMarkup(marks.deskBadge, colour)}</g>`;
  if (marks.question) s += `<g data-mark="question">${unknownMarkup(pose !== "standing" ? -66 : -72)}</g>`;
  return s;
}

/** The Your turn ring on the floor under the figure, at `(dx, dy)` from
 *  its origin (a seated figure's floor point is its seat's). */
export function underMarkup(ring: boolean, dx: number, dy: number): string {
  return ring ? `<ellipse fill="none" stroke="${paint("color-attention")}" cx="${dx}" cy="${dy}" rx="25" ry="12.5" stroke-width="2.6"/>` : "";
}
