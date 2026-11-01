// The furnished floor (AD-169, AD-173), ported from the prototype v2's
// "props and plants" and "static drawing": the floor layer's shapes, drawn
// by `SceneFloor.svelte`, and the statics of the object layer, markup that
// `objects.ts` sorts by depth. Pure: every input is a number, a geometry
// constant or a closed type, never session text, and every detail that
// varies comes from a seed (an office's from its slot), never from
// `Math.random`, so an office looks the same on every start.
import type { ProjectColor } from "../generated/ProjectColor";
import { PROJECT_COLORS, projectColor } from "../colors";
import {
  BASE_COLUMNS,
  BEACON,
  CORR_Y,
  FLOOR_DEPTH,
  HW,
  M,
  PART,
  PT,
  R,
  SHARED_CELLS,
  SLAB,
  WALL,
  WHITEBOARD,
  floorWidth,
  officeCells,
  projector,
  roomOrigin,
  type Cell,
  type Projector,
  type SharedZone,
} from "./geometry";
import { COUNTER, DECIDER, officeCell, officeSpots, sharedSpots } from "./places";

// ------------------------------------------------------------ paints

/** Every token of app.css the drawing paints with; tokens.test.ts holds
 *  each one to its declaration there. */
export const PAINTS = [
  "color-surface",
  "color-working",
  "color-text",
  "color-text-muted",
  "color-attention",
  "color-attention-text",
  "color-attention-border",
  "color-alert",
  "color-on-alert",
  "scene-ground",
  "scene-slab-l",
  "scene-slab-r",
  "scene-room",
  "scene-room-edge",
  "scene-room-vacant",
  "scene-alert-floor",
  "scene-wall-l",
  "scene-wall-r",
  "scene-wall-top",
  "scene-skirting",
  "scene-window",
  "scene-board",
  "scene-part-t",
  "scene-part-l",
  "scene-part-r",
  "scene-wood-t",
  "scene-wood-l",
  "scene-wood-r",
  "scene-metal",
  "scene-metal-2",
  "scene-screen-off",
  "scene-screen-line",
  "scene-chair",
  "scene-chair-2",
  "scene-sofa-t",
  "scene-sofa-l",
  "scene-sofa-r",
  "scene-rug",
  "scene-rug-2",
  "scene-leaf",
  "scene-leaf-2",
  "scene-leaf-3",
  "scene-pot",
  "scene-pot-shade",
  "scene-pot-rim",
  "scene-soil",
  "scene-ceramic",
  "scene-ceramic-shade",
  "scene-lamp",
  "scene-shadow",
  "figure-tunic",
  "figure-tunic-shade",
  "orch-tunic",
  "orch-tunic-shade",
  "figure-legs",
  "figure-shoe",
  "figure-eye",
  "figure-rim",
] as const;
export type Paint = (typeof PAINTS)[number];

/** A token as a fill or stroke value. SVG presentation attributes take
 *  `var()` in Chromium; inline `style` attributes would break the window's
 *  CSP (`style-src 'self'`). */
export const paint = (token: Paint): string => `var(--${token})`;

/** Translucent overlays that shade a face of any colour. */
export const SHADE_1 = "rgb(0 0 0 / 0.1)";
export const SHADE_2 = "rgb(0 0 0 / 0.22)";
export const HIGHLIGHT = "rgb(255 255 255 / 0.14)";

/** An office's colour: its project's, or the neutral one. */
export const projectPaint = (colour: ProjectColor | null): string => projectColor(colour);

const PROJECT_PAINTS = Object.values(PROJECT_COLORS);

// ------------------------------------------------------------ seeds

/** The prototype's seeded generator: the same seed gives the same draws. */
export function mulberry32(seed: number): () => number {
  let a = seed;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** 32-bit FNV-1a of `text`'s UTF-16 code units. */
export function fnv1a(text: string): number {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

const pick = <T>(rng: () => number, list: readonly T[]): T => list[Math.floor(rng() * list.length) % list.length]!;

/** An office's seed: from its slot, never from session data (D-478). */
export function officeSeed(slot: number): number {
  return fnv1a(`office${slot}`) ^ 0x51ed;
}

// ------------------------------------------------------------ markup

const r1 = (n: number): number => Math.round(n * 10) / 10;
type XY = readonly [number, number];

function at(p: Projector, gx: number, gy: number, z = 0): XY {
  const { x, y } = p(gx, gy, z);
  return [x, y];
}

const pts = (list: readonly XY[]): string => list.map(([x, y]) => `${r1(x)},${r1(y)}`).join(" ");

function poly(list: readonly XY[], fill: string, extra = ""): string {
  return `<polygon fill="${fill}" points="${pts(list)}"${extra}/>`;
}

type Faces3 = readonly [top: string, left: string, right: string];

function faces(p: Projector, gx: number, gy: number, w: number, d: number, z0: number, h: number) {
  return {
    top: [at(p, gx, gy, z0 + h), at(p, gx + w, gy, z0 + h), at(p, gx + w, gy + d, z0 + h), at(p, gx, gy + d, z0 + h)],
    left: [at(p, gx, gy + d, z0), at(p, gx + w, gy + d, z0), at(p, gx + w, gy + d, z0 + h), at(p, gx, gy + d, z0 + h)],
    right: [at(p, gx + w, gy, z0), at(p, gx + w, gy + d, z0), at(p, gx + w, gy + d, z0 + h), at(p, gx + w, gy, z0 + h)],
  };
}

function box(p: Projector, gx: number, gy: number, w: number, d: number, z0: number, h: number, [t, l, r]: Faces3, topExtra = ""): string {
  const f = faces(p, gx, gy, w, d, z0, h);
  return poly(f.left, l) + poly(f.right, r) + poly(f.top, t, topExtra);
}

const WOOD: Faces3 = [paint("scene-wood-t"), paint("scene-wood-l"), paint("scene-wood-r")];
const METAL: Faces3 = [paint("scene-metal-2"), paint("scene-metal"), paint("scene-metal")];
const SOFA: Faces3 = [paint("scene-sofa-t"), paint("scene-sofa-l"), paint("scene-sofa-r")];
const SURFACE = paint("color-surface");

// ------------------------------------------------------------ props and plants

function cyl(x: number, y: number, rb: number, rt: number, h: number, fl: string, fr: string, ft: string, ry = 0.38): string {
  const yt = y - h;
  let s = `<path fill="${fl}" d="M${r1(x - rb)} ${r1(y)} A${r1(rb)} ${r1(rb * ry)} 0 0 0 ${r1(x)} ${r1(y + rb * ry)} L${r1(x)} ${r1(yt + rt * ry)} A${r1(rt)} ${r1(rt * ry)} 0 0 1 ${r1(x - rt)} ${r1(yt)} Z"/>`;
  s += `<path fill="${fr}" d="M${r1(x)} ${r1(y + rb * ry)} A${r1(rb)} ${r1(rb * ry)} 0 0 0 ${r1(x + rb)} ${r1(y)} L${r1(x + rt)} ${r1(yt)} A${r1(rt)} ${r1(rt * ry)} 0 0 1 ${r1(x)} ${r1(yt + rt * ry)} Z"/>`;
  s += `<ellipse fill="${ft}" cx="${r1(x)}" cy="${r1(yt)}" rx="${r1(rt)}" ry="${r1(rt * ry)}"/>`;
  return s;
}

function leaf(cx: number, cy: number, rx: number, ry: number, rot: number, fill: string): string {
  return `<ellipse fill="${fill}" cx="${r1(cx)}" cy="${r1(cy)}" rx="${rx}" ry="${ry}" transform="rotate(${Math.round(rot)} ${r1(cx)} ${r1(cy)})"/>`;
}

export type PlantKind = "ficus" | "snake" | "bushy";
const PLANT_KINDS: readonly PlantKind[] = ["ficus", "snake", "bushy"];

/** A pot with volume and layered foliage, grown from its seed. */
function plant(p: Projector, kind: PlantKind, gx: number, gy: number, seed: number): string {
  const rng = mulberry32(seed);
  const [x0, y0] = at(p, gx, gy);
  let s = `<g transform="translate(${r1(x0)} ${r1(y0)})"><ellipse fill="${paint("scene-shadow")}" cx="0" cy="1" rx="14" ry="5.5"/>`;
  const ceramic = rng() < 0.4;
  const pot = ceramic
    ? [paint("scene-ceramic"), paint("scene-ceramic-shade"), paint("scene-ceramic")]
    : [paint("scene-pot"), paint("scene-pot-shade"), paint("scene-pot-rim")];
  const potH = kind === "snake" ? 17 : 15;
  const rb = kind === "snake" ? 6.4 : 7.5;
  const rt = kind === "snake" ? 7.4 : 9.5;
  s += cyl(0, 0, rb, rt, potH, pot[0]!, pot[1]!, pot[2]!);
  s += `<ellipse fill="${paint("scene-soil")}" cx="0" cy="${r1(-potH + 0.6)}" rx="${r1(rt - 1.4)}" ry="${r1((rt - 1.4) * 0.36)}"/>`;
  const top = -potH;
  if (kind === "ficus") {
    s += `<path fill="${paint("scene-wood-r")}" d="M-1.3 ${top} C-2.5 ${top - 14} 2.4 ${top - 22} -0.4 ${top - 40} L1.4 ${top - 40} C4 ${top - 22} -0.6 ${top - 14} 1.3 ${top} Z"/>`;
    s += `<path stroke="${paint("scene-wood-r")}" d="M0.4 ${top - 22} Q-5 ${top - 27} -8 ${top - 31}" fill="none" stroke-width="1.2"/><path stroke="${paint("scene-wood-r")}" d="M0.6 ${top - 28} Q6 ${top - 32} 8 ${top - 37}" fill="none" stroke-width="1.2"/>`;
    const layers: [Paint, number][] = [
      ["scene-leaf-2", 22],
      ["scene-leaf", 20],
      ["scene-leaf-3", 12],
    ];
    layers.forEach(([token, n], li) => {
      for (let i = 0; i < n; i++) {
        const a = rng() * Math.PI * 2;
        const rr = Math.sqrt(rng());
        const cx = Math.cos(a) * 14 * rr - li * 0.8;
        const cy = top - 40 + Math.sin(a) * 13 * rr - li * 1.6;
        s += leaf(cx, cy, 4.6, 2.5, rng() * 180, paint(token));
      }
    });
  } else if (kind === "snake") {
    const n = 6 + Math.floor(rng() * 2);
    for (let i = 0; i < n; i++) {
      const x = -5 + (10 * i) / (n - 1) + (rng() - 0.5) * 1.5;
      const h = 24 + rng() * 22;
      const lean = (rng() - 0.5) * 9;
      const w = 2.4 + rng() * 1.1;
      const fill = paint(i % 2 ? "scene-leaf-2" : "scene-leaf");
      s += `<path fill="${fill}" d="M${r1(x - w)} ${top} Q${r1(x - w + lean * 0.4)} ${r1(top - h * 0.6)} ${r1(x + lean)} ${r1(top - h)} Q${r1(x + w + lean * 0.4)} ${r1(top - h * 0.6)} ${r1(x + w)} ${top} Z"/>`;
      s += `<path fill="${paint("scene-leaf-3")}" opacity="0.75" d="M${r1(x - 0.6)} ${top} Q${r1(x + lean * 0.4)} ${r1(top - h * 0.55)} ${r1(x + lean)} ${r1(top - h + 2)} Q${r1(x + 0.8 + lean * 0.4)} ${r1(top - h * 0.55)} ${r1(x + 0.6)} ${top} Z"/>`;
    }
  } else {
    const layers: [Paint, number, number, number][] = [
      ["scene-leaf-2", 16, 15, 10],
      ["scene-leaf", 14, 12, 14],
      ["scene-leaf-3", 9, 9, 17],
    ];
    for (const [token, n, spread, lift] of layers) {
      for (let i = 0; i < n; i++) {
        const a = Math.PI + rng() * Math.PI;
        const rr = 0.4 + rng() * 0.6;
        s += leaf(Math.cos(a) * spread * rr, top - lift + Math.sin(a) * 9 * rr, 5.4, 3.4, (a * 180) / Math.PI + 90, paint(token));
      }
    }
    for (const side of [-1, 1]) {
      s += leaf(side * 10, top + 3, 4.6, 2.8, side * 60, paint("scene-leaf-2")) + leaf(side * 11.5, top + 8, 4, 2.4, side * 75, paint("scene-leaf"));
    }
  }
  return s + "</g>";
}

/** A small succulent in a pot; (x, y) is the screen point on the desk top. */
function succulent(x: number, y: number, rng: () => number): string {
  let s = cyl(x, y, 3.2, 3.8, 5, paint("scene-ceramic"), paint("scene-ceramic-shade"), paint("scene-soil"));
  const fill = paint(rng() < 0.5 ? "scene-leaf" : "scene-leaf-3");
  for (let i = 0; i < 7; i++) {
    const a = (i / 7) * Math.PI * 2;
    s += leaf(x + Math.cos(a) * 2.6, y - 6.2 + Math.sin(a) * 1.1, 2.4, 1.3, (a * 180) / Math.PI, i % 2 ? paint("scene-leaf-2") : fill);
  }
  return s + `<circle fill="${paint("scene-leaf-3")}" cx="${r1(x)}" cy="${r1(y - 7)}" r="1.4"/>`;
}

function mug(x: number, y: number, fill: string): string {
  return (
    cyl(x, y, 2.6, 2.6, 5, fill, paint("scene-chair-2"), paint("scene-metal-2")) +
    `<path stroke="${paint("scene-chair-2")}" d="M${r1(x + 2.6)} ${r1(y - 4)} q2.6 0.6 0 2.8" fill="none" stroke-width="1.2"/>`
  );
}

/** A shelf of books. "gy": standing along gx, books facing +gy; "gx":
 *  along gy, books facing +gx. */
function bookshelf(p: Projector, gx: number, gy: number, w: number, d: number, h: number, side: "gx" | "gy", rng: () => number): string {
  let s = box(p, gx, gy, w, d, 0, h, WOOD);
  const books = [...PROJECT_PAINTS, paint("scene-chair"), SURFACE, paint("scene-wood-r")];
  const levels = Math.max(2, Math.floor(h / 0.42));
  for (let k = 0; k < levels; k++) {
    const z0 = 0.08 + (k * (h - 0.12)) / levels;
    const zMax = (h - 0.12) / levels - 0.06;
    const len = side === "gy" ? w : d;
    let a = 0.05;
    while (a < len - 0.08) {
      const bw = 0.05 + rng() * 0.07;
      const bh = Math.min(zMax, 0.16 + rng() * 0.16);
      if (rng() < 0.12) {
        a += bw + 0.04;
        continue;
      }
      const fill = pick(rng, books);
      const q: XY[] =
        side === "gy"
          ? [at(p, gx + a, gy + d + 0.005, z0), at(p, gx + a + bw, gy + d + 0.005, z0), at(p, gx + a + bw, gy + d + 0.005, z0 + bh), at(p, gx + a, gy + d + 0.005, z0 + bh)]
          : [at(p, gx + w + 0.005, gy + a, z0), at(p, gx + w + 0.005, gy + a + bw, z0), at(p, gx + w + 0.005, gy + a + bw, z0 + bh), at(p, gx + w + 0.005, gy + a, z0 + bh)];
      s += poly(q, fill);
      a += bw + 0.01;
    }
    const plank: XY[] =
      side === "gy"
        ? [at(p, gx, gy + d + 0.006, z0 - 0.04), at(p, gx + w, gy + d + 0.006, z0 - 0.04), at(p, gx + w, gy + d + 0.006, z0), at(p, gx, gy + d + 0.006, z0)]
        : [at(p, gx + w + 0.006, gy, z0 - 0.04), at(p, gx + w + 0.006, gy + d, z0 - 0.04), at(p, gx + w + 0.006, gy + d, z0), at(p, gx + w + 0.006, gy, z0)];
    s += poly(plank, paint("scene-wood-r"));
  }
  return s;
}

function cabinet(p: Projector, gx: number, gy: number, w: number, d: number, h: number): string {
  let s = box(p, gx, gy, w, d, 0, h, [paint("scene-ceramic"), paint("scene-ceramic-shade"), paint("scene-chair")]);
  const n = Math.max(2, Math.round(h / 0.32));
  for (let i = 0; i < n; i++) {
    const z0 = 0.06 + (i * (h - 0.08)) / n;
    const z1 = z0 + (h - 0.08) / n - 0.04;
    s += poly([at(p, gx + 0.04, gy + d + 0.004, z0), at(p, gx + w - 0.04, gy + d + 0.004, z0), at(p, gx + w - 0.04, gy + d + 0.004, z1), at(p, gx + 0.04, gy + d + 0.004, z1)], paint("scene-ceramic"), ' opacity="0.7"');
    const zc = (z0 + z1) / 2;
    s += poly([at(p, gx + w / 2 - 0.08, gy + d + 0.006, zc - 0.02), at(p, gx + w / 2 + 0.08, gy + d + 0.006, zc - 0.02), at(p, gx + w / 2 + 0.08, gy + d + 0.006, zc + 0.02), at(p, gx + w / 2 - 0.08, gy + d + 0.006, zc + 0.02)], paint("scene-metal-2"));
  }
  return s;
}

function printer(p: Projector, gx: number, gy: number): string {
  let s = cabinet(p, gx, gy, 0.6, 0.4, 0.55);
  s += box(p, gx + 0.05, gy + 0.04, 0.5, 0.32, 0.55, 0.2, [SURFACE, paint("scene-ceramic-shade"), paint("scene-chair")]);
  s += box(p, gx + 0.15, gy + 0.1, 0.3, 0.16, 0.75, 0.02, [SURFACE, SURFACE, SURFACE]);
  return s;
}

function floorLamp(p: Projector, gx: number, gy: number): string {
  const [x, y] = at(p, gx, gy);
  let s = `<ellipse fill="${paint("scene-shadow")}" cx="${r1(x)}" cy="${r1(y)}" rx="9" ry="3.6"/><ellipse fill="${paint("scene-metal")}" cx="${r1(x)}" cy="${r1(y - 0.5)}" rx="6" ry="2.4"/>`;
  s += `<rect fill="${paint("scene-metal")}" x="${r1(x - 0.9)}" y="${r1(y - 52)}" width="1.8" height="52"/>`;
  s += `<ellipse fill="${paint("scene-lamp")}" cx="${r1(x)}" cy="${r1(y - 46)}" rx="16" ry="11" opacity="0.45"/>`;
  s += cyl(x, y - 46, 8.5, 5, 12, SURFACE, paint("scene-ceramic-shade"), paint("scene-ceramic"));
  return s;
}

function coatRack(p: Projector, gx: number, gy: number): string {
  const [x, y] = at(p, gx, gy);
  let s = `<ellipse fill="${paint("scene-shadow")}" cx="${r1(x)}" cy="${r1(y)}" rx="9" ry="3.6"/><path stroke="${paint("scene-metal")}" d="M${r1(x - 6)} ${r1(y)} L${r1(x)} ${r1(y - 4)} L${r1(x + 6)} ${r1(y)}" fill="none" stroke-width="1.6"/>`;
  s += `<rect fill="${paint("scene-wood-r")}" x="${r1(x - 1)}" y="${r1(y - 56)}" width="2" height="54"/>`;
  s += `<path fill="${paint("scene-chair")}" d="M${r1(x - 1)} ${r1(y - 52)} q-8 4 -7 22 l5 1 q-1 -12 3 -20 Z"/>`;
  s += `<path fill="${projectColor("blue")}" d="M${r1(x + 1)} ${r1(y - 50)} q7 5 5 18 l-4 1 q1 -10 -2 -16 Z" opacity="0.9"/>`;
  return s + `<circle fill="${paint("scene-wood-r")}" cx="${r1(x)}" cy="${r1(y - 57)}" r="2.2"/>`;
}

/** A sofa or an armchair: plinth on short legs, cushions, armrests. Its
 *  near armrest comes apart, so it can be drawn in front of whoever sits
 *  next to it. "gy": back rest along the back, facing +gy; "gx": along the
 *  left, facing +gx. */
function sofa(p: Projector, gx: number, gy: number, len: number, side: "gx" | "gy", seats: number) {
  const D = 0.74;
  const BT = 0.2;
  const AW = 0.16;
  const main: string[] = [];
  const along = (a0: number, a1: number, d0: number, d1: number, z0: number, h: number, f: Faces3) =>
    side === "gy" ? box(p, gx + a0, gy + d0, a1 - a0, d1 - d0, z0, h, f) : box(p, gx + d0, gy + a0, d1 - d0, a1 - a0, z0, h, f);
  for (const a of [0.04, len - 0.1]) for (const dd of [0.06, D - 0.12]) main.push(along(a, a + 0.06, dd, dd + 0.06, 0, 0.06, WOOD));
  main.push(along(0, len, 0, D, 0.06, 0.14, [paint("scene-sofa-l"), paint("scene-sofa-r"), paint("scene-sofa-r")]));
  main.push(along(0, len, 0, BT, 0.2, 0.98, SOFA));
  const sw = len / seats;
  for (let i = 0; i < seats; i++) {
    main.push(along(i * sw + 0.02, (i + 1) * sw - 0.02, BT + 0.02, BT + 0.24, 0.38, 0.68, SOFA));
    main.push(along(i * sw + 0.02, (i + 1) * sw - 0.02, BT, D, 0.2, 0.18, SOFA));
    const hi: XY[] =
      side === "gy"
        ? [at(p, gx + i * sw + 0.08, gy + BT + 0.06, 0.381), at(p, gx + (i + 1) * sw - 0.08, gy + BT + 0.06, 0.381), at(p, gx + (i + 1) * sw - 0.08, gy + D - 0.08, 0.381), at(p, gx + i * sw + 0.08, gy + D - 0.08, 0.381)]
        : [at(p, gx + BT + 0.06, gy + i * sw + 0.08, 0.381), at(p, gx + D - 0.08, gy + i * sw + 0.08, 0.381), at(p, gx + D - 0.08, gy + (i + 1) * sw - 0.08, 0.381), at(p, gx + BT + 0.06, gy + (i + 1) * sw - 0.08, 0.381)];
    main.push(poly(hi, HIGHLIGHT));
  }
  main.unshift(along(-AW, 0, 0, D, 0.06, 0.62, SOFA));
  const near = along(len, len + AW, 0, D, 0.06, 0.62, SOFA);
  const nearAnchor: XY = side === "gy" ? [gx + len + AW / 2, gy + D / 2] : [gx + D / 2, gy + len + AW / 2];
  return { main: main.join(""), near, nearAnchor };
}

/** What a desk's corner may hold. */
const DESK_PROPS = ["mug", "papers", "lamp", "succulent", "books", "none"] as const;

/** A desk with a drawer pedestal, a keyboard and seeded props; (gx, gy) is
 *  its chair, the desk stands behind it. Its screens carry `data-screen`
 *  and their lines `data-lines`, which `objects.ts` lights. */
function desk(p: Projector, gx: number, gy: number, colour: string, rng: () => number): string {
  const x0 = gx - 0.6;
  const y0 = gy - 1.08;
  const pedLeft = rng() < 0.5;
  let s = "";
  const px = pedLeft ? x0 + 0.04 : x0 + 0.8;
  s += box(p, px, y0 + 0.06, 0.36, 0.46, 0, 0.68, WOOD);
  for (let i = 0; i < 3; i++) {
    const z0 = 0.06 + i * 0.2;
    s += poly([at(p, px + 0.05, y0 + 0.521, z0), at(p, px + 0.31, y0 + 0.521, z0), at(p, px + 0.31, y0 + 0.521, z0 + 0.16), at(p, px + 0.05, y0 + 0.521, z0 + 0.16)], paint("scene-wood-t"), ' opacity="0.6"');
  }
  const lx = pedLeft ? x0 + 1.1 : x0 + 0.04;
  s += box(p, lx, y0 + 0.06, 0.06, 0.06, 0, 0.68, METAL) + box(p, lx, y0 + 0.46, 0.06, 0.06, 0, 0.68, METAL);
  s += box(p, x0, y0, 1.2, 0.58, 0.68, 0.06, WOOD);
  s += poly([at(p, x0, gy - 0.5, 0.62), at(p, x0 + 1.2, gy - 0.5, 0.62), at(p, x0 + 1.2, gy - 0.5, 0.74), at(p, x0, gy - 0.5, 0.74)], colour);
  const dual = rng() < 0.3;
  const monitors: [number, number][] = dual ? [[gx - 0.2, 0.84], [gx + 0.38, 0.5]] : [[gx + (rng() - 0.5) * 0.1, 0.84]];
  for (const [mx, w] of monitors) {
    s += box(p, mx - 0.08, gy - 0.98, 0.16, 0.1, 0.74, 0.16, METAL);
    s += box(p, mx - w / 2, gy - 1.02, w, 0.08, 0.86, 0.6, METAL);
    const q: XY[] = [at(p, mx - w / 2 + 0.06, gy - 0.94, 0.92), at(p, mx + w / 2 - 0.06, gy - 0.94, 0.92), at(p, mx + w / 2 - 0.06, gy - 0.94, 1.4), at(p, mx - w / 2 + 0.06, gy - 0.94, 1.4)];
    s += poly(q, paint("scene-screen-off"), " data-screen");
    s += `<g data-lines display="none">`;
    const k2 = w / 0.84;
    for (const [z, a, b] of [[0.38, -0.3, 0.12], [0.3, -0.3, 0.2], [0.22, -0.22, 0.02], [0.14, -0.3, 0.06]] as const) {
      const [x1, y1] = at(p, mx + a * k2, gy - 0.94, 0.92 + z);
      const [x2, y2] = at(p, mx + b * k2, gy - 0.94, 0.92 + z);
      s += `<line stroke="${paint("scene-screen-line")}" x1="${r1(x1)}" y1="${r1(y1)}" x2="${r1(x2)}" y2="${r1(y2)}" stroke-width="2" stroke-linecap="round"/>`;
    }
    s += "</g>";
  }
  s += box(p, gx - 0.24, gy - 0.78, 0.48, 0.14, 0.74, 0.02, [paint("scene-ceramic"), paint("scene-chair"), paint("scene-chair-2")]);
  const corners: XY[] = dual ? [[gx - 0.48, gy - 0.66]] : [[gx - 0.46, gy - 0.66], [gx + 0.44, gy - 0.66]];
  for (const [ax, ay] of corners) {
    const kind = pick(rng, DESK_PROPS);
    const [x, y] = at(p, ax, ay, 0.74);
    if (kind === "mug") s += mug(x, y, pick(rng, [SURFACE, paint("color-working"), paint("scene-ceramic")]));
    else if (kind === "papers") {
      s += box(p, ax - 0.12, ay - 0.1, 0.22, 0.17, 0.74, 0.02, [SURFACE, paint("scene-ceramic-shade"), paint("scene-ceramic-shade")]);
      s += box(p, ax - 0.1, ay - 0.08, 0.22, 0.17, 0.76, 0.01, [SURFACE, SURFACE, SURFACE]);
    } else if (kind === "lamp") {
      s += `<ellipse fill="${paint("scene-metal")}" cx="${r1(x)}" cy="${r1(y)}" rx="4" ry="1.6"/><path stroke="${paint("scene-metal")}" d="M${r1(x)} ${r1(y)} L${r1(x - 2)} ${r1(y - 12)} L${r1(x + 5)} ${r1(y - 17)}" fill="none" stroke-width="1.6"/>`;
      s += `<path fill="${paint("scene-metal-2")}" d="M${r1(x + 1)} ${r1(y - 19)} L${r1(x + 9)} ${r1(y - 16)} L${r1(x + 7)} ${r1(y - 12)} Z"/><ellipse fill="${paint("scene-lamp")}" cx="${r1(x + 8)}" cy="${r1(y - 9)}" rx="6" ry="3" opacity="0.5"/>`;
    } else if (kind === "succulent") s += succulent(x, y, rng);
    else if (kind === "books") {
      s += box(p, ax - 0.12, ay - 0.09, 0.24, 0.16, 0.74, 0.05, [paint("scene-wood-t"), projectColor("blue"), paint("scene-chair")]);
      s += box(p, ax - 0.1, ay - 0.08, 0.2, 0.14, 0.79, 0.05, [SURFACE, projectColor("terracotta"), paint("scene-chair-2")]);
    }
  }
  return s;
}

/** A desk chair seen from behind, in its office's colour; its `data-away`
 *  group holds the number of a kept desk whose session is elsewhere. */
function chair(x: number, y: number, colour: string): string {
  return (
    `<ellipse fill="${paint("scene-shadow")}" cx="${x}" cy="${y}" rx="15" ry="6.5"/>` +
    `<path stroke="${paint("scene-chair-2")}" d="M${x - 9} ${y + 1} L${x} ${y - 2} L${x + 9} ${y + 1} M${x} ${y - 2} L${x} ${y + 3}" stroke-width="1.6" fill="none" stroke-linecap="round"/>` +
    `<rect fill="${paint("scene-chair-2")}" x="${r1(x - 1.4)}" y="${r1(y - 13)}" width="2.8" height="11"/>` +
    `<rect x="${r1(x - 11)}" y="${r1(y - 37)}" width="22" height="24" rx="7" fill="${colour}"/><rect fill="${SHADE_1}" x="${r1(x - 11)}" y="${r1(y - 37)}" width="22" height="24" rx="7"/>` +
    `<ellipse cx="${x}" cy="${r1(y - 13)}" rx="12.5" ry="5.5" fill="${colour}"/><ellipse fill="${HIGHLIGHT}" cx="${x - 1}" cy="${r1(y - 14)}" rx="8" ry="3"/>` +
    `<g data-away></g>`
  );
}

// ------------------------------------------------------------ floor layer

/** The alert room's floor and its hazard outline, which `SceneFloor.svelte`
 *  paints by the room's state (AD-175). */
export type AlertPart = "floor" | "hazard";

export type FloorShape =
  | { kind: "polygon"; points: string; fill: string; stroke?: string; strokeWidth?: number; opacity?: number; dash?: string; alert?: AlertPart }
  | { kind: "line"; x1: number; y1: number; x2: number; y2: number; stroke: string; strokeWidth: number; opacity?: number; round?: boolean }
  | { kind: "ellipse"; cx: number; cy: number; rx: number; ry: number; fill: string; stroke?: string; strokeWidth?: number; dash?: string; opacity?: number }
  | { kind: "circle"; cx: number; cy: number; r: number; fill: string; stroke?: string; strokeWidth?: number };

const polygon = (list: readonly XY[], fill: string, more: Partial<Extract<FloorShape, { kind: "polygon" }>> = {}): FloorShape => ({
  kind: "polygon",
  points: pts(list),
  fill,
  ...more,
});

function line(a: XY, b: XY, stroke: string, strokeWidth: number, more: { opacity?: number; round?: boolean } = {}): FloorShape {
  return { kind: "line", x1: r1(a[0]), y1: r1(a[1]), x2: r1(b[0]), y2: r1(b[1]), stroke, strokeWidth, ...more };
}

/** A room's floor outline, back, right, front and left corners. */
export function roomOutline(columns: number, cell: Cell): string {
  const p = projector(columns);
  const { gx, gy } = roomOrigin(cell);
  return pts([at(p, gx, gy), at(p, gx + R, gy), at(p, gx + R, gy + R), at(p, gx, gy + R)]);
}

function walls(p: Projector, fx: number): FloorShape[] {
  const fy = FLOOR_DEPTH;
  const t = 0.22;
  return [
    polygon([at(p, 0, 0), at(p, 0, fy), at(p, 0, fy, WALL), at(p, 0, 0, WALL)], paint("scene-wall-l")),
    polygon([at(p, 0, 0), at(p, fx, 0), at(p, fx, 0, WALL), at(p, 0, 0, WALL)], paint("scene-wall-r")),
    polygon([at(p, 0, 0), at(p, 0, fy), at(p, 0, fy, 0.16), at(p, 0, 0, 0.16)], paint("scene-skirting")),
    polygon([at(p, 0, 0), at(p, fx, 0), at(p, fx, 0, 0.16), at(p, 0, 0, 0.16)], paint("scene-skirting")),
    polygon([at(p, -t, -t, WALL), at(p, fx, -t, WALL), at(p, fx, 0, WALL), at(p, 0, 0, WALL), at(p, 0, fy, WALL), at(p, -t, fy, WALL)], paint("scene-wall-top")),
    polygon([at(p, 0, fy), at(p, -t, fy), at(p, -t, fy, WALL), at(p, 0, fy, WALL)], paint("scene-slab-l")),
    polygon([at(p, fx, 0), at(p, fx, -t), at(p, fx, -t, WALL), at(p, fx, 0, WALL)], paint("scene-slab-r")),
  ];
}

/** A point on the left wall ("l", along gy) or the back wall ("r", along gx). */
function wallPoint(p: Projector, side: "l" | "r", a: number, z: number, off = 0): XY {
  return side === "l" ? at(p, off, a, z) : at(p, a, off, z);
}

function wallWindow(p: Projector, side: "l" | "r", a0: number, a1: number, z0: number, z1: number): FloorShape[] {
  const W = (a: number, z: number, off = 0) => wallPoint(p, side, a, z, off);
  const i = 0.07;
  const mid = (a0 + a1) / 2;
  return [
    polygon([W(a0, z0), W(a1, z0), W(a1, z1), W(a0, z1)], paint("scene-wall-top")),
    polygon([W(a0 + i, z0 + i * 1.4), W(a1 - i, z0 + i * 1.4), W(a1 - i, z1 - i * 1.4), W(a0 + i, z1 - i * 1.4)], paint("scene-window")),
    line(W(mid, z0), W(mid, z1), paint("scene-wall-top"), 3),
    polygon([W(a0 - 0.08, z0 - 0.08), W(a1 + 0.08, z0 - 0.08), W(a1 + 0.08, z0 - 0.08, 0.12), W(a0 - 0.08, z0 - 0.08, 0.12)], paint("scene-wood-t")),
  ];
}

/** A framed picture: seeded abstract art in the project colours. */
function wallFrame(p: Projector, side: "l" | "r", a0: number, a1: number, z0: number, z1: number, seed: number): FloorShape[] {
  const rng = mulberry32(seed);
  const W = (a: number, z: number, off = 0.01) => wallPoint(p, side, a, z, off);
  const out = [
    polygon([W(a0 - 0.05, z0 - 0.05), W(a1 + 0.05, z0 - 0.05), W(a1 + 0.05, z1 + 0.05), W(a0 - 0.05, z1 + 0.05)], paint("scene-wood-r")),
    polygon([W(a0, z0), W(a1, z0), W(a1, z1), W(a0, z1)], paint("scene-board")),
  ];
  const colours = [...PROJECT_PAINTS, paint("scene-leaf")];
  for (let i = 0; i < 3; i++) {
    const u0 = a0 + 0.08 + rng() * (a1 - a0) * 0.5;
    const u1 = Math.min(a1 - 0.06, u0 + 0.12 + rng() * (a1 - a0) * 0.4);
    const v0 = z0 + 0.08 + rng() * (z1 - z0) * 0.4;
    const v1 = Math.min(z1 - 0.06, v0 + 0.1 + rng() * (z1 - z0) * 0.4);
    out.push(polygon([W(u0, v0, 0.012), W(u1, v0, 0.012), W(u1, v1, 0.012), W(u0, v1, 0.012)], pick(rng, colours), { opacity: 0.85 }));
  }
  return out;
}

function door(p: Projector, gy0: number, gy1: number): FloorShape[] {
  const H = 2.2;
  const [kx, ky] = at(p, 0.47, gy1 - 0.3, 1.0);
  return [
    polygon([at(p, 0, gy0 - 0.1), at(p, 0, gy1 + 0.1), at(p, 0, gy1 + 0.1, H + 0.12), at(p, 0, gy0 - 0.1, H + 0.12)], paint("scene-wood-r")),
    polygon([at(p, 0, gy0), at(p, 0, gy1), at(p, 0, gy1, H), at(p, 0, gy0, H)], paint("scene-metal-2")),
    polygon([at(p, 0, gy0 + 0.08, 0.2), at(p, 0, gy1 - 0.08, 0.2), at(p, 0, gy1 - 0.08, H - 0.1), at(p, 0, gy0 + 0.08, H - 0.1)], paint("scene-window"), { opacity: 0.35 }),
    polygon([at(p, 0, gy1), at(p, 0.55, gy1 - 0.35), at(p, 0.55, gy1 - 0.35, H), at(p, 0, gy1, H)], paint("scene-wood-l")),
    { kind: "circle", cx: r1(kx), cy: r1(ky), r: 1.8, fill: paint("scene-metal") },
  ];
}

/** The whiteboard above the decision office, static in v0.4. */
function whiteboard(p: Projector, gx0: number, gx1: number, z0: number, z1: number): FloorShape[] {
  const W = (a: number, z: number) => at(p, a, WHITEBOARD.gy, z);
  const out = [
    polygon([W(gx0 - 0.08, z0 - 0.08), W(gx1 + 0.08, z0 - 0.08), W(gx1 + 0.08, z1 + 0.08), W(gx0 - 0.08, z1 + 0.08)], paint("scene-wood-r")),
    polygon([W(gx0, z0), W(gx1, z0), W(gx1, z1), W(gx0, z1)], paint("scene-board")),
  ];
  [0.6, 0.45, 0.7, 0.35].forEach((len, i) => {
    const z = z1 - 0.3 - i * 0.3;
    const a = gx0 + 0.25;
    out.push(line(W(a, z), W(a + 0.18, z), PROJECT_PAINTS[i % PROJECT_PAINTS.length]!, 5, { round: true }));
    out.push(line(W(a + 0.4, z), W(a + 0.4 + len * (gx1 - gx0 - 0.9), z), paint("color-text-muted"), 2, { round: true }));
  });
  out.push(polygon([W(gx0 + 0.3, z0 - 0.02), W(gx1 - 0.3, z0 - 0.02), W(gx1 - 0.3, z0 + 0.12), W(gx0 + 0.3, z0 + 0.12)], paint("scene-metal-2")));
  return out;
}

/** The wall clock's fixed time: it is drawn, never read (AD-64, AD-169). */
export const WALL_CLOCK = { hours: 10, minutes: 10 } as const;

function wallClock(p: Projector): FloorShape[] {
  const [cx, cy] = at(p, 3.75, 0, 2.45);
  const hand = (turns: number, length: number): XY => [cx + Math.sin(turns * 2 * Math.PI) * length, cy - Math.cos(turns * 2 * Math.PI) * length];
  const minute = WALL_CLOCK.minutes / 60;
  const hour = ((WALL_CLOCK.hours % 12) + minute) / 12;
  return [
    { kind: "circle", cx: r1(cx), cy: r1(cy), r: 14, fill: SURFACE, stroke: paint("scene-wall-top"), strokeWidth: 2 },
    line([cx, cy], hand(hour, 7), paint("color-text"), 2.4, { round: true }),
    line([cx, cy], hand(minute, 10.5), paint("color-text"), 1.6, { round: true }),
    { kind: "circle", cx: r1(cx), cy: r1(cy), r: 1.6, fill: paint("color-text") },
  ];
}

function room(p: Projector, cell: Cell, lx0: number, ly0: number, lx1: number, ly1: number): XY[] {
  const { gx, gy } = roomOrigin(cell);
  return [at(p, gx + lx0, gy + ly0), at(p, gx + lx1, gy + ly0), at(p, gx + lx1, gy + ly1), at(p, gx + lx0, gy + ly1)];
}

function rug(p: Projector, cell: Cell, lx0: number, ly0: number, lx1: number, ly1: number, token: Paint): FloorShape[] {
  const i = 0.14;
  return [
    polygon(room(p, cell, lx0, ly0, lx1, ly1), paint(token)),
    polygon(room(p, cell, lx0 + i, ly0 + i, lx1 - i, ly1 - i), "none", { stroke: SURFACE, strokeWidth: 1.5, opacity: 0.6 }),
  ];
}

function seams(p: Projector, cell: Cell): FloorShape[] {
  const { gx, gy } = roomOrigin(cell);
  const out: FloorShape[] = [];
  for (let i = 1; i < R; i++) {
    out.push(line(at(p, gx + i, gy), at(p, gx + i, gy + R), paint("scene-room-edge"), 1, { opacity: 0.55 }));
    out.push(line(at(p, gx, gy + i), at(p, gx + R, gy + i), paint("scene-room-edge"), 1, { opacity: 0.55 }));
  }
  return out;
}

/** A window and a frame on the back wall behind `column`, the frame
 *  seeded by its column. */
function backWallColumn(p: Projector, column: number): FloorShape[] {
  const { gx } = roomOrigin([column, 0]);
  return [...wallWindow(p, "r", gx + 0.9, gx + 2.7, 1.25, 2.65), ...wallFrame(p, "r", gx + 3.4, gx + 4.15, 1.5, 2.35, 13 + column)];
}

/** The alert room's hazard outline: `inset` tiles inside its floor, drawn
 *  while the room is lit (the prototype v2's `.hazard`). */
export const HAZARD = {
  inset: 0.12,
  stroke: { stroke: paint("color-alert"), strokeWidth: 4, dash: "10 8", opacity: 0.85 },
} as const;

/** The paints of the alert room's floor while it is lit. */
export const ALERT_FLOOR = { fill: paint("scene-alert-floor"), stroke: paint("color-alert") } as const;

/** Layer 1 for a floor of `columns` columns: ground, slab, walls and what
 *  hangs on them, the corridor, room floors and seams, rugs, the alert
 *  room's hazard outline. The waiting places are not marked on the floor
 *  (D-1046): people waiting read as a queue without them. */
export function floorShapes(columns: number): FloorShape[] {
  const p = projector(columns);
  const fx = floorWidth(columns);
  const fy = FLOOR_DEPTH;
  const out: FloorShape[] = [];
  // No shadow under the floor (D-544): the prototype's ellipse sat under
  // the slab's middle, lit from nowhere the furniture is, and the slab's
  // two shaded sides already set the floor on the page.
  out.push(polygon([at(p, 0, fy), at(p, fx, fy), at(p, fx, fy, -SLAB), at(p, 0, fy, -SLAB)], paint("scene-slab-l")));
  out.push(polygon([at(p, fx, 0), at(p, fx, fy), at(p, fx, fy, -SLAB), at(p, fx, 0, -SLAB)], paint("scene-slab-r")));
  out.push(polygon([at(p, 0, 0), at(p, fx, 0), at(p, fx, fy), at(p, 0, fy)], paint("scene-ground")));
  out.push(...walls(p, fx));
  out.push(polygon([at(p, M + 0.6, CORR_Y - 0.3), at(p, fx - M, CORR_Y - 0.3), at(p, fx - M, CORR_Y + 0.3), at(p, M + 0.6, CORR_Y + 0.3)], paint("scene-room-vacant")));
  out.push(...door(p, M + 1.0, M + 2.2));
  out.push(...wallWindow(p, "l", 7.0, 9.4, 1.25, 2.65));
  out.push(...wallFrame(p, "l", 3.5, 4.25, 1.4, 2.2, 11));
  out.push(...wallFrame(p, "l", 9.75, 10.4, 1.5, 2.3, 12));
  out.push(...wallWindow(p, "r", 0.9, 2.5, 1.25, 2.65));
  out.push(...whiteboard(p, WHITEBOARD.gx0, WHITEBOARD.gx1, WHITEBOARD.z0, WHITEBOARD.z1));
  out.push(...wallWindow(p, "r", 12.4, 14.2, 1.25, 2.65));
  out.push(...wallFrame(p, "r", 14.9, 15.7, 1.45, 2.35, 13));
  out.push(...wallWindow(p, "r", 17.9, 19.7, 1.25, 2.65));
  out.push(...wallFrame(p, "r", 20.2, 20.75, 1.6, 2.4, 14));
  // The prototype furnished the base floor's back wall by hand; the
  // columns the floor gains repeat its pattern.
  for (let column = BASE_COLUMNS; column < columns; column++) out.push(...backWallColumn(p, column));
  out.push(...wallClock(p));
  const cells: Cell[] = [...Object.values(SHARED_CELLS), ...officeCells(columns)];
  for (const cell of cells) {
    const alert = cell === SHARED_CELLS.alert ? { alert: "floor" as const } : {};
    out.push(polygon(room(p, cell, 0, 0, R, R), paint("scene-room"), { stroke: paint("scene-room-edge"), strokeWidth: 2, ...alert }));
    out.push(...seams(p, cell));
  }
  out.push(polygon(room(p, SHARED_CELLS.reception, -0.05, 0.95, 0.9, 2.25), paint("scene-metal-2"), { opacity: 0.35 }));
  out.push(...rug(p, SHARED_CELLS.decision, 0.95, 2.2, 3.85, 3.15, "scene-rug"));
  out.push(...rug(p, SHARED_CELLS.lounge, 1.35, 1.5, 3.0, 2.95, "scene-rug-2"));
  out.push(polygon(room(p, SHARED_CELLS.alert, HAZARD.inset, HAZARD.inset, R - HAZARD.inset, R - HAZARD.inset), "none", { ...HAZARD.stroke, alert: "hazard" }));
  return out;
}

// ------------------------------------------------------------ statics

/** A static of the object layer: markup and its depth `gx + gy`. */
export interface Static {
  markup: string;
  depth: number;
  /** An office's desk or chair, which `objects.ts` dims and lights. */
  desk?: { slot: number; index: number; part: "desk" | "chair" };
  /** A partition of a room, whose top `objects.ts` may colour. */
  partition?: SharedZone | "office";
}

/** What an office cell holds: a project's desks, More offices, or nothing
 *  yet (furnished in the neutral colour, every desk free). */
export type OfficeUse = { kind: "office"; colour: ProjectColor | null } | { kind: "more" };

/** Where each room's partitions open, along its left side ("x", along gy)
 *  and its back side ("y", along gx), in tiles. */
const DOORS: Record<SharedZone, { x: [number, number][]; y: [number, number][] }> = {
  reception: { x: [[1.0, 2.2]], y: [] },
  decision: { x: [[0.55, 1.45]], y: [] },
  lounge: { x: [], y: [[3.0, 4.0]] },
  alert: { x: [], y: [[1.7, 2.7]] },
};
/** A front-row office opens on the corridor through its back side. */
const OFFICE_DOOR: [number, number] = [1.7, 2.8];

function partitions(p: Projector, cell: Cell, gaps: { x: [number, number][]; y: [number, number][] }, owner: SharedZone | "office"): Static[] {
  const { gx: ox, gy: oy } = roomOrigin(cell);
  const faces3: Faces3 = [paint("scene-part-t"), paint("scene-part-l"), paint("scene-part-r")];
  const pieces = (from: number, to: number, openings: [number, number][]) => {
    const out: [number, number][] = [];
    let a = from;
    for (const [g0, g1] of [...openings].sort((u, v) => u[0] - v[0])) {
      if (g0 > a) out.push([a, g0]);
      a = g1;
    }
    if (a < to) out.push([a, to]);
    const split: [number, number][] = [];
    for (const [u, v] of out) {
      const n = Math.max(1, Math.ceil(v - u));
      for (let i = 0; i < n; i++) split.push([u + ((v - u) * i) / n, u + ((v - u) * (i + 1)) / n]);
    }
    return split;
  };
  const out: Static[] = [];
  for (const [a, b] of pieces(0, R, gaps.x)) {
    out.push({ markup: box(p, ox - PT, oy + a, PT, b - a, 0, PART, faces3, " data-top"), depth: ox - PT / 2 + oy + (a + b) / 2, partition: owner });
  }
  for (const [a, b] of pieces(-PT, R, gaps.y)) {
    out.push({ markup: box(p, ox + a, oy - PT, b - a, PT, 0, PART, faces3, " data-top"), depth: ox + (a + b) / 2 + oy - PT / 2, partition: owner });
  }
  return out;
}

const local = (cell: Cell, lx: number, ly: number): XY => {
  const { gx, gy } = roomOrigin(cell);
  return [gx + lx, gy + ly];
};

function addPlant(out: Static[], p: Projector, kind: PlantKind, [gx, gy]: XY, seed: number): void {
  out.push({ markup: plant(p, kind, gx, gy, seed), depth: gx + gy });
}

function receptionStatics(p: Projector): Static[] {
  const cell = SHARED_CELLS.reception;
  const out: Static[] = [];
  const [gx, gy] = local(cell, COUNTER.lx, COUNTER.ly);
  let s = box(p, gx, gy, COUNTER.w, COUNTER.d, 0, 0.95, WOOD);
  s += box(p, gx - 0.05, gy - 0.05, 2.1, 0.65, 0.95, 0.07, [paint("scene-wood-t"), paint("scene-wood-r"), paint("scene-wood-r")]);
  s += poly([at(p, gx, gy + 0.55, 0.62), at(p, gx + 2.0, gy + 0.55, 0.62), at(p, gx + 2.0, gy + 0.55, 0.74), at(p, gx, gy + 0.55, 0.74)], paint("color-working"));
  s += box(p, gx + 1.35, gy + 0.08, 0.5, 0.08, 1.02, 0.38, METAL);
  s += box(p, gx + 0.35, gy + 0.18, 0.36, 0.26, 1.02, 0.05, [SURFACE, paint("scene-chair"), paint("scene-chair-2")]);
  const [mx, my] = at(p, gx + 1.05, gy + 0.35, 1.02);
  s += succulent(mx, my, mulberry32(7));
  out.push({ markup: s, depth: gx + 1.0 + gy + 0.25 });
  addPlant(out, p, "ficus", local(cell, 0.45, 0.45), 101);
  addPlant(out, p, "snake", local(cell, 4.0, 3.95), 102);
  const [cx, cy] = local(cell, 0.35, 3.5);
  out.push({ markup: coatRack(p, cx, cy), depth: cx + cy });
  return out;
}

/** The decider's desk and chair, drawn empty (D-503), the queue's rail. */
function decisionStatics(p: Projector): Static[] {
  const cell = SHARED_CELLS.decision;
  const out: Static[] = [];
  const [cx, cy] = local(cell, DECIDER.chair.lx, DECIDER.chair.ly);
  const [dx, dy] = local(cell, DECIDER.desk.lx, DECIDER.desk.ly);
  const { w, d } = DECIDER.desk;
  let s = box(p, dx, dy, w, d, 0.68, 0.06, WOOD);
  s += box(p, dx + 0.04, dy + 0.04, 0.36, 0.5, 0, 0.68, WOOD) + box(p, dx + w - 0.4, dy + 0.04, 0.36, 0.5, 0, 0.68, WOOD);
  s += poly([at(p, dx, cy - 0.5, 0.62), at(p, dx + w, cy - 0.5, 0.62), at(p, dx + w, cy - 0.5, 0.74), at(p, dx, cy - 0.5, 0.74)], paint("color-working"));
  const sc = DECIDER.screens;
  for (const off of sc.offsets) {
    const mx = cx + off;
    const sy = cy + sc.dy;
    s += box(p, mx - 0.08, sy - 0.04, 0.16, 0.1, 0.74, 0.14, METAL);
    s += box(p, mx - 0.48, sy - 0.08, 0.96, 0.08, 0.86, 0.74, METAL);
    s += poly([at(p, mx - sc.half, sy, sc.z0), at(p, mx + sc.half, sy, sc.z0), at(p, mx + sc.half, sy, sc.z1), at(p, mx - sc.half, sy, sc.z1)], paint("scene-screen-off"));
  }
  s += box(p, cx - 0.28, cy - 0.78, 0.56, 0.14, 0.74, 0.02, [paint("scene-ceramic"), paint("scene-chair"), paint("scene-chair-2")]);
  const [mx, my] = at(p, cx + 0.78, cy - 0.68, 0.74);
  s += mug(mx, my, SURFACE);
  out.push({ markup: s, depth: cx + cy - 0.6 });
  const [x, y] = at(p, cx, cy);
  out.push({ markup: chair(r1(x), r1(y), paint("color-working")), depth: cx + cy - 0.05 });
  const posts = [-0.05, 0.95, 1.95];
  posts.forEach((post, i) => {
    const [gx, gy] = local(cell, post, 3.3);
    let rail = box(p, gx - 0.05, gy - 0.05, 0.1, 0.1, 0, 0.9, METAL) + box(p, gx - 0.12, gy - 0.12, 0.24, 0.24, 0, 0.05, METAL);
    const next = posts[i + 1];
    if (next !== undefined) {
      const [hx] = local(cell, next, 3.3);
      rail += poly([at(p, gx, gy, 0.7), at(p, hx, gy, 0.7), at(p, hx, gy, 0.82), at(p, gx, gy, 0.82)], paint("color-working"));
    }
    out.push({ markup: rail, depth: gx + gy });
  });
  addPlant(out, p, "bushy", local(cell, 4.1, 3.95), 201);
  return out;
}

/** The paints of the alert room's parts that light, quiet then lit
 *  (AD-175). `objects.ts` writes them when the room's state changes. */
export const ALERT_PAINTS = {
  lamp: { quiet: "scene-screen-off", lit: "color-alert" },
  panel: { quiet: "scene-screen-off", lit: "color-alert" },
  mark: { quiet: "scene-wall-top", lit: "color-on-alert" },
  partition: { quiet: "scene-part-t", lit: "color-alert" },
} as const satisfies Record<string, { quiet: Paint; lit: Paint }>;

/** The beam's opacity while lit: drawn static, the prototype's rotation
 *  left out, since FR-9 forbids an animation as long as the alert (D-474). */
export const BEAM_OPACITY = 0.22;

/** The alert room: its beacon and status board, drawn quiet, their lit
 *  parts marked for `objects.ts`, and a plant. The beacon's glow is layer
 *  2's. */
function alertStatics(p: Projector): Static[] {
  const cell = SHARED_CELLS.alert;
  const out: Static[] = [];
  const [bx, by] = local(cell, BEACON.lx, BEACON.ly);
  let s = box(p, bx - 0.2, by - 0.2, 0.4, 0.4, 0, 0.08, METAL) + box(p, bx - 0.04, by - 0.04, 0.08, 0.08, 0.08, 1.75, METAL);
  const [x, y] = at(p, bx, by, BEACON.z);
  s += `<path data-beacon-beam display="none" fill="${paint("color-alert")}" opacity="${BEAM_OPACITY}" d="M${r1(x)} ${r1(y)} L${r1(x + 60)} ${r1(y - 16)} L${r1(x + 60)} ${r1(y + 16)} Z"/>`;
  s += `<rect fill="${paint("scene-metal")}" x="${r1(x - 9)}" y="${r1(y + 2)}" width="18" height="6" rx="2"/>`;
  s += `<path data-beacon-lamp fill="${paint(ALERT_PAINTS.lamp.quiet)}" stroke="${paint("scene-metal")}" stroke-width="1.2" d="M${r1(x - 8)} ${r1(y + 3)} L${r1(x - 8)} ${r1(y - 4)} A8 8 0 0 1 ${r1(x + 8)} ${r1(y - 4)} L${r1(x + 8)} ${r1(y + 3)} Z"/>`;
  s += `<ellipse fill="${SURFACE}" cx="${r1(x - 3)}" cy="${r1(y - 6)}" rx="2" ry="3" opacity="0.6"/>`;
  out.push({ markup: s, depth: bx + by });
  const [sx, sy] = local(cell, 3.3, 0.35);
  let board = box(p, sx - 0.05, sy - 0.05, 0.1, 0.1, 0, 1.1, METAL) + box(p, sx + 0.75, sy - 0.05, 0.1, 0.1, 0, 1.1, METAL);
  board += box(p, sx - 0.1, sy, 1.0, 0.08, 1.0, 0.75, METAL);
  board += poly([at(p, sx - 0.02, sy + 0.09, 1.07), at(p, sx + 0.82, sy + 0.09, 1.07), at(p, sx + 0.82, sy + 0.09, 1.68), at(p, sx - 0.02, sy + 0.09, 1.68)], paint(ALERT_PAINTS.panel.quiet), " data-status-panel");
  // The "!" drawn, not written: no text node in this layer.
  const [tx, ty] = at(p, sx + 0.4, sy + 0.09, 1.22);
  board += `<g data-status-mark fill="${paint(ALERT_PAINTS.mark.quiet)}"><rect x="${r1(tx - 1.6)}" y="${r1(ty - 12)}" width="3.2" height="8.5" rx="1.4"/><circle cx="${r1(tx)}" cy="${r1(ty - 1)}" r="1.8"/></g>`;
  out.push({ markup: board, depth: sx + 0.4 + sy + 0.05 });
  addPlant(out, p, "snake", local(cell, 4.0, 3.95), 301);
  return out;
}

function loungeStatics(p: Projector): Static[] {
  const cell = SHARED_CELLS.lounge;
  const out: Static[] = [];
  const addSofa = (lx: number, ly: number, len: number, side: "gx" | "gy", seats: number, anchor: XY) => {
    const [gx, gy] = local(cell, lx, ly);
    const s = sofa(p, gx, gy, len, side, seats);
    out.push({ markup: s.main, depth: gx + anchor[0] + gy + anchor[1] });
    out.push({ markup: s.near, depth: s.nearAnchor[0] + s.nearAnchor[1] });
  };
  addSofa(0.5, 0.12, 2.1, "gy", 3, [1.05, 0.3]);
  addSofa(0.12, 1.6, 1.4, "gx", 2, [0.3, 0.7]);
  for (const ly of [2.0, 3.1]) addSofa(3.25, ly, 0.6, "gy", 1, [0.3, 0.25]);
  {
    const [gx, gy] = local(cell, 1.75, 1.75);
    let s = "";
    for (const [a, b] of [[0.04, 0.04], [0.75, 0.04], [0.04, 0.68], [0.75, 0.68]] as const) s += box(p, gx + a, gy + b, 0.06, 0.06, 0, 0.32, WOOD);
    s += box(p, gx, gy, 0.85, 0.78, 0.32, 0.06, WOOD);
    const [m1x, m1y] = at(p, gx + 0.25, gy + 0.3, 0.38);
    s += mug(m1x, m1y, SURFACE);
    const [m2x, m2y] = at(p, gx + 0.6, gy + 0.55, 0.38);
    s += mug(m2x, m2y, paint("color-working"));
    s += box(p, gx + 0.42, gy + 0.12, 0.3, 0.22, 0.38, 0.015, [projectColor("blue"), paint("scene-chair"), paint("scene-chair")]);
    out.push({ markup: s, depth: gx + 0.42 + gy + 0.39 });
  }
  {
    const [gx, gy] = local(cell, 0.15, 3.5);
    let s = box(p, gx, gy, 0.72, 0.72, 0, 0.85, WOOD);
    s += box(p, gx + 0.12, gy + 0.14, 0.46, 0.42, 0.85, 0.6, METAL);
    s += box(p, gx + 0.12, gy + 0.56, 0.46, 0.04, 1.2, 0.2, [paint("scene-screen-off"), paint("color-working"), paint("scene-metal")]);
    s += box(p, gx + 0.27, gy + 0.48, 0.12, 0.12, 0.85, 0.12, [SURFACE, paint("scene-chair"), paint("scene-chair-2")]);
    const [m1x, m1y] = at(p, gx + 0.62, gy + 0.2, 0.85);
    s += mug(m1x, m1y, SURFACE);
    out.push({ markup: s, depth: gx + 0.36 + gy + 0.36 });
  }
  const [bx, by] = local(cell, 0.02, 0.1);
  out.push({ markup: bookshelf(p, bx, by, 0.26, 1.25, 1.75, "gx", mulberry32(401)), depth: bx + 0.13 + by + 0.4 });
  const [lx, ly] = local(cell, 2.88, 0.32);
  out.push({ markup: floorLamp(p, lx, ly), depth: lx + ly });
  addPlant(out, p, "ficus", local(cell, 4.05, 0.5), 402);
  addPlant(out, p, "bushy", local(cell, 4.05, 4.05), 403);
  return out;
}

/** An office's chairs, room-local, in desk order: its desk spots of
 *  `places.ts`, the same in every office. */
const CHAIR_POINTS: readonly XY[] = (() => {
  const origin = roomOrigin(officeCell(0));
  return officeSpots(0).desks.map((s) => [s.gx - origin.gx, s.gy - origin.gy] as const);
})();

/** An office's statics: four desks and chairs in its colour, one seeded
 *  extra piece and a plant; `slot` seeds the details, `cell` places them. */
export function officeStatics(p: Projector, slot: number, cell: Cell, colour: ProjectColor | null): Static[] {
  const rng = mulberry32(officeSeed(slot));
  const fill = projectPaint(colour);
  const out: Static[] = [];
  CHAIR_POINTS.forEach(([lx, ly], index) => {
    const [gx, gy] = local(cell, lx, ly);
    out.push({ markup: desk(p, gx, gy, fill, rng), depth: gx + gy - 0.6, desk: { slot, index, part: "desk" } });
    const [x, y] = at(p, gx, gy);
    out.push({ markup: chair(r1(x), r1(y), fill), depth: gx + gy - 0.05, desk: { slot, index, part: "chair" } });
  });
  const extra = pick(rng, ["bookshelf", "cabinet", "printer", "bookshelf"] as const);
  const [ex, ey] = local(cell, 1.92, 0.04);
  if (extra === "bookshelf") out.push({ markup: bookshelf(p, ex, ey, 0.66, 0.3, 1.25 + rng() * 0.4, "gy", rng), depth: ex + 0.33 + ey + 0.15 });
  else if (extra === "cabinet") out.push({ markup: cabinet(p, ex, ey, 0.6, 0.38, 0.95), depth: ex + 0.3 + ey + 0.19 });
  else out.push({ markup: printer(p, ex, ey), depth: ex + 0.3 + ey + 0.2 });
  const corner = pick(rng, [[4.05, 0.45], [4.05, 4.05], [0.42, 4.05]] as const);
  addPlant(out, p, pick(rng, PLANT_KINDS), local(cell, corner[0], corner[1]), fnv1a(`office${slot}`) ^ 0x77);
  return out;
}

/** More offices: a room of shelves, no desk, since its sessions have no
 *  place of their own (D-103). */
function moreStatics(p: Projector, slot: number, cell: Cell): Static[] {
  const rng = mulberry32(officeSeed(slot));
  const out: Static[] = [];
  const [ex, ey] = local(cell, 0.6, 0.04);
  out.push({ markup: bookshelf(p, ex, ey, 1.4, 0.3, 1.45, "gy", rng), depth: ex + 0.7 + ey + 0.15 });
  const [cx, cy] = local(cell, 2.4, 0.04);
  out.push({ markup: cabinet(p, cx, cy, 0.6, 0.38, 0.95), depth: cx + 0.3 + cy + 0.19 });
  addPlant(out, p, pick(rng, PLANT_KINDS), local(cell, 4.05, 4.05), fnv1a(`office${slot}`) ^ 0x77);
  return out;
}

/** Every static of a floor of `columns` columns, sorted by depth; `uses`
 *  says what each office slot holds (a slot left out is not assigned). */
export function buildStatics(columns: number, uses: readonly (OfficeUse | undefined)[]): Static[] {
  const p = projector(columns);
  const out: Static[] = [];
  for (const zone of Object.keys(SHARED_CELLS) as SharedZone[]) out.push(...partitions(p, SHARED_CELLS[zone], DOORS[zone], zone));
  officeCells(columns).forEach((cell) => out.push(...partitions(p, cell, { x: [], y: cell[1] === 1 ? [OFFICE_DOOR] : [] }, "office")));
  out.push(...receptionStatics(p), ...decisionStatics(p), ...alertStatics(p), ...loungeStatics(p));
  officeCells(columns).forEach((cell, slot) => {
    const use = uses[slot] ?? { kind: "office", colour: null };
    out.push(...(use.kind === "more" ? moreStatics(p, slot, cell) : officeStatics(p, slot, cell, use.colour)));
  });
  return out.sort((a, b) => a.depth - b.depth);
}
