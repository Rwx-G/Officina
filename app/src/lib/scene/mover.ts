// The mover (AD-172 as amended, D-469, D-470, D-483, D-502): between the
// layout, which names places, and `objects.ts`, which draws figures. It
// walks each figure along the navigation graph to its new place, glides it
// there when walking is off, and places it at once under reduced motion, on
// the first layout of a run, and when the window is hidden or a frame comes
// late. The scene's one frame loop runs here, only while something moves,
// and the scene element's `data-moving` says whether it runs. Ported from
// the prototype v2's "movement" (`setGoal`, `advance`, `stepGlide`,
// `arrive`) and its frame loop.
// The orchestrator (AD-179, D-568) is one more walker, at the decider's
// chair; it walks only to brief: when a layout names a briefing, the
// employee detours by reception's counter and the orchestrator comes to
// the counter's staff side, both hold `BRIEFING_DWELL_MS`, then each goes
// on to its place. Once per work order, and never when a walk would not
// be drawn.
import { HH, HW } from "./geometry";
import { ORCHESTRATOR_KEY, type Employee, type Helper, type Marks, type Pose, type Role, type SceneLayout, type Staff } from "./layout";
import { allSpots, briefingSpot, navGraph, restPoint, staffSpots, type Facing, type NavGraph, type Spot } from "./places";
import { route, smooth, type RoutePoint, type RouteStart } from "./paths";

/** One frame of the walk cycle: the legs' and the arms' angles in degrees,
 *  in opposition, and the bob in px. */
export interface Stride {
  leg: number;
  arm: number;
  bob: number;
}

/** The walk (AD-172): speeds in tiles per second, accelerations in tiles
 *  per second squared, about 1.3 m/s with a tile of about 0.7 m. A walker
 *  never crawls below `vmin` before its spot; legs and arms swing in
 *  opposition by `swing` degrees with a `bob` px bob, one half-cycle per
 *  `step` tiles; facing follows the screen velocity smoothed over
 *  `facing.smoothing` s, above `facing.minSpeed` px/s, and changes only
 *  past `facing.horizontal` and `facing.vertical` of the speed; on a seat
 *  it pauses `sitPause` s, then sits, sliding in over `slide` s. */
export const WALK = {
  vmax: 1.9,
  vmin: 0.3,
  acc: 4.5,
  dec: 3.0,
  step: 0.5,
  swing: { leg: 24, arm: 20 },
  bob: 1.4,
  facing: { smoothing: 0.12, minSpeed: 4, horizontal: 0.3, vertical: 0.22 },
  sitPause: 0.35,
  slide: 0.3,
} as const;

/** A glide with walking off: a straight ease-in-out move, in ms. */
export const GLIDE_MS = 320;
/** A fade in on arrival through the entrance, or out on leaving, in ms. */
export const FADE_MS = 320;
/** A sub-agent whose key left the layout stays this long, in ms, before it
 *  fades out: a resumed sub-agent's stop and next start can reach the window
 *  on two boards, and it must not blink between them (D-548). */
export const HELPER_HOLD_MS = 1000;
/** The longest frame the walk advances by, in s: a slower frame slows the
 *  walk rather than letting it jump. */
export const FRAME_CAP_S = 0.1;
/** A frame later than this, in s, means the window was hidden or stalled:
 *  every walker is placed at once and every fade ends (D-502). */
export const FRAME_GAP_S = 0.25;
/** The mover draws at most one frame per this many ms of frame start time
 *  (story 4.16, D-551): on a display faster than about 77 Hz it lets
 *  refreshes pass, so a 240 Hz screen draws 60 frames a second and a 144 Hz
 *  one 72, while a 60 or 75 Hz one draws every refresh. The frames' start
 *  times are refresh-aligned, so a 75 Hz screen's 13.3 ms never reads as
 *  less. The step per drawn frame stays under AD-70's 3 px: at `WALK.vmax`
 *  the fastest screen direction moves about 97 px/s, 1.6 px at 60 frames a
 *  second and 1.9 at the 50 a 100 Hz screen gets. */
export const MIN_FRAME_MS = 13;
/** How long the employee and the orchestrator stand at the counter once
 *  both are there, in ms (D-568). */
export const BRIEFING_DWELL_MS = 1600;
/** The walk advances in steps no longer than this, in s. */
const SUBSTEP_S = 0.05;

/** What the mover needs from the page; tests inject their own. */
export interface MotionEnv {
  /** A monotonic clock, in ms. */
  now(): number;
  /** Calls `frame` at the next frame with its start time on `now()`'s
   *  clock, which a browser aligns on the display's refresh. */
  request(frame: (at: number) => void): number;
  cancel(handle: number): void;
  hidden(): boolean;
  /** Calls `listener` when the page becomes hidden; returns the unsubscribe. */
  onHidden(listener: () => void): () => void;
  /** The system's reduced motion, read at each move, never stored. */
  reducedMotion(): boolean;
}

/** The page's frame loop, clock, visibility and motion setting. */
export function browserEnv(): MotionEnv {
  return {
    now: () => performance.now(),
    request: (frame) => requestAnimationFrame((at) => frame(at)),
    cancel: (handle) => cancelAnimationFrame(handle),
    hidden: () => document.visibilityState === "hidden",
    onHidden: (listener) => {
      const changed = () => {
        if (document.visibilityState === "hidden") listener();
      };
      document.addEventListener("visibilitychange", changed);
      return () => document.removeEventListener("visibilitychange", changed);
    },
    reducedMotion: () => globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false,
  };
}

/** One figure as `objects.ts` draws it in this frame. */
export interface FigureFrame {
  key: string;
  project: string;
  /** Its floor point on the grid: its depth and its ring's centre. */
  gx: number;
  gy: number;
  /** Where it is drawn on the grid: its floor point, or on its way into a
   *  seat. */
  drawGx: number;
  drawGy: number;
  moving: boolean;
  pose: Pose;
  facing: Facing;
  /** Turned to face left. */
  flip: boolean;
  stride: Stride | null;
  opacity: number;
  marks: Marks;
  alerted: boolean;
  /** A sub-agent beside its lead's desk (AD-176): smaller, without a cap,
   *  never walking. */
  helper: boolean;
  /** The orchestrator's figure takes its own rig (D-567). */
  role: Role;
}

const NO_MARKS: Marks = { ticket: null, approval: false, deskBadge: null, hand: false, ring: false, question: false, received: false };

/** The orchestrator as the mover walks it: an employee without a card,
 *  marks or alert. */
function staffEmployee(s: Staff): Employee {
  return { key: s.key, zone: "decision", project: "", spot: s.spot, group: null, at: s.at, pose: s.pose, facing: s.facing, marks: NO_MARKS, alert: null, briefing: null };
}

const roleOf = (key: string): Role => (key === ORCHESTRATOR_KEY ? "orchestrator" : "employee");

/** The spots the mover places figures at: every employee's, and the
 *  orchestrator's chair and counter place. */
function moverSpots(columns: number): Map<string, Spot> {
  const { chair, counter } = staffSpots();
  return new Map([...allSpots(columns), chair, counter].map((s) => [s.id, s]));
}

/** A sub-agent standing at its place, at `opacity`. */
function helperFrame(h: Helper, opacity: number): FigureFrame {
  return {
    key: h.key,
    project: h.project,
    gx: h.gx,
    gy: h.gy,
    drawGx: h.gx,
    drawGy: h.gy,
    moving: false,
    pose: "standing",
    facing: "front",
    flip: false,
    stride: null,
    opacity,
    marks: NO_MARKS,
    alerted: false,
    helper: true,
    role: "employee",
  };
}

/** A figure on its way shows no mark but the question mark (the prototype
 *  v2's `poseOf`): its ticket and ring belong to the place it walks to. */
export function walkingMarks(marks: Marks): Marks {
  return { ticket: null, approval: false, deskBadge: null, hand: false, ring: false, question: marks.question, received: false };
}

/** Every placed employee at rest at its place: the mover's output once
 *  nothing moves and nobody has walked yet. */
export function restingFrames(layout: SceneLayout): Map<string, FigureFrame> {
  const spots = moverSpots(layout.columns);
  const out = new Map<string, FigureFrame>();
  const placed = layout.orchestrator ? [...layout.employees, staffEmployee(layout.orchestrator)] : layout.employees;
  for (const e of placed) {
    const spot = e.spot === null ? undefined : spots.get(e.spot);
    if (!spot) continue;
    const rest = restPoint(spot);
    out.set(e.key, {
      key: e.key,
      project: e.project,
      gx: spot.gx,
      gy: spot.gy,
      drawGx: rest.gx,
      drawGy: rest.gy,
      moving: false,
      pose: e.pose,
      facing: e.facing,
      flip: false,
      stride: null,
      opacity: 1,
      marks: e.marks,
      alerted: e.alert !== null,
      helper: false,
      role: roleOf(e.key),
    });
  }
  for (const h of layout.helpers) out.set(h.key, helperFrame(h, 1));
  return out;
}

interface Walker {
  key: string;
  /** The last layout entry of its key. */
  employee: Employee;
  /** The node it heads for or stands on: its spot's, or the exit. */
  goal: string;
  gx: number;
  gy: number;
  /** The node it stands on, `null` between nodes. */
  at: string | null;
  route: RoutePoint[] | null;
  /** The route's raw nodes, `null` for a start between nodes. */
  nodes: (string | null)[];
  /** Distance walked along the route, the route point before it, speed. */
  s: number;
  k: number;
  v: number;
  glide: { x0: number; y0: number; x1: number; y1: number; t0: number } | null;
  /** The nodes it walked from and toward, for a route from mid-walk. */
  prev: string | null;
  next: string | null;
  /** Smoothed screen velocity, px/s. */
  svx: number;
  svy: number;
  phase: number;
  flip: boolean;
  view: Facing;
  /** When it sits, in ms; `null` for a standing place. */
  sitAt: number | null;
  fade: { t0: number; from: number; to: 0 | 1 } | null;
  /** Gone from the layout's places: it walks out, or fades where it is. */
  leaving: boolean;
  /** Its frame once at rest, handed out again, the same object, until a
   *  layout or a settle changes it: `objects.ts` skips a frame it drew. */
  still: FigureFrame | null;
  /** On a briefing's detour, the spot it goes on to afterwards; `goal` is
   *  then the counter. `null` otherwise. */
  home: string | null;
}

/** The briefing under way: the employee's key, and when both stand at the
 *  counter, the end of their dwell. */
interface Detour {
  key: string;
  dwellUntil: number | null;
}

/** A sub-agent's figure: it never walks, it fades in where it stands and
 *  fades out there after `HELPER_HOLD_MS`. */
interface Standing {
  helper: Helper;
  /** A fade out starts at `t0`, which is in the future during the hold. */
  fade: { t0: number; from: number; to: 0 | 1 } | null;
  leaving: boolean;
  /** Its frame once its fade has ended, handed out again as a walker's
   *  `still`. */
  still: FigureFrame | null;
}

const SLIDE_MS = WALK.slide * 1000;
const clamp01 = (n: number): number => Math.min(1, Math.max(0, n));
const easeInOut = (t: number): number => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);

function strideOf(phase: number): Stride {
  const s = Math.sin(phase);
  return {
    leg: Math.round(WALK.swing.leg * s),
    arm: Math.round(-WALK.swing.arm * s),
    bob: Math.round(-WALK.bob * Math.abs(s) * 10) / 10,
  };
}

export class Mover {
  readonly #env: MotionEnv;
  readonly #walkers = new Map<string, Walker>();
  readonly #helpers = new Map<string, Standing>();
  /** The live keys of the last layout: a key outside it is new. */
  #seen = new Set<string>();
  /** The work orders already briefed, or let go unbriefed, this mount. */
  readonly #briefed = new Set<string>();
  /** Briefings waiting their turn: one at the counter at a time. */
  #queue: { orderId: string; key: string }[] = [];
  #detour: Detour | null = null;
  #started = false;
  #walking = true;
  #columns = -1;
  #graph: NavGraph | null = null;
  #spots = new Map<string, Spot>();
  #handle: number | null = null;
  #last = 0;
  /** The start time of the last frame drawn: a loop that starts again
   *  after a rest finds it long past, so its first frame draws. */
  #lastFrameAt = Number.NEGATIVE_INFINITY;
  #host: Element | null = null;
  #moving = false;
  #transit = "";
  readonly #listeners = new Set<(figures: ReadonlyMap<string, FigureFrame>) => void>();
  readonly #offHidden: () => void;
  /** Told the keys walking or gliding to a place whenever they change. */
  onTransit: ((keys: ReadonlySet<string>) => void) | null = null;

  constructor(env: MotionEnv = browserEnv()) {
    this.#env = env;
    this.#offHidden = env.onHidden(() => {
      this.#settleAll();
      this.#afterChange(this.#env.now());
      this.#notify();
    });
  }

  /** The scene element, whose `data-moving` the mover keeps. */
  attach(host: Element): void {
    this.#host = host;
    const value = this.#moving ? "1" : "0";
    if (host.getAttribute("data-moving") !== value) host.setAttribute("data-moving", value);
  }

  /** `listener` gets the figures after each frame and each settle that a
   *  new layout did not cause; returns the unsubscribe. */
  subscribe(listener: (figures: ReadonlyMap<string, FigureFrame>) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  get moving(): boolean {
    return this.#moving;
  }

  destroy(): void {
    if (this.#handle !== null) this.#env.cancel(this.#handle);
    this.#handle = null;
    this.#offHidden();
    this.#listeners.clear();
    this.#walkers.clear();
    this.#helpers.clear();
    this.#queue = [];
    this.#detour = null;
  }

  /** The keys on their way to a place, a briefing's detour included. */
  transit(): Set<string> {
    const out = new Set<string>();
    for (const w of this.#walkers.values()) if (!w.leaving && (w.route || w.glide || w.home !== null)) out.add(w.key);
    return out;
  }

  /** Takes a new layout: each figure whose place changed starts toward
   *  it, a new key comes in, a departed one goes. The caller draws
   *  `figures()` after it. */
  update(layout: SceneLayout, walking: boolean): void {
    if (layout.columns !== this.#columns) {
      this.#graph = navGraph(layout.columns);
      this.#spots = moverSpots(layout.columns);
      this.#columns = layout.columns;
    }
    const now = this.#env.now();
    for (const w of this.#walkers.values()) w.still = null;
    // Turning walking off sends every walker to its place at once (AD-66).
    if (this.#walking && !walking) this.#settleAll();
    this.#walking = walking;
    const instant = !this.#started || this.#env.reducedMotion() || this.#env.hidden();
    const placed = new Map<string, { e: Employee; spot: Spot }>();
    const everyone = layout.orchestrator ? [...layout.employees, staffEmployee(layout.orchestrator)] : layout.employees;
    for (const e of everyone) {
      const spot = e.spot === null ? undefined : this.#spots.get(e.spot);
      if (spot) placed.set(e.key, { e, spot });
    }
    const live = new Set(layout.employees.map((e) => e.key));
    for (const w of [...this.#walkers.values()]) {
      // The orchestrator fades where it is: it never walks out (D-568).
      if (!placed.has(w.key)) this.#leave(w, live.has(w.key) || w.key === ORCHESTRATOR_KEY, instant, now);
    }
    for (const { e, spot } of placed.values()) {
      const w = this.#walkers.get(e.key);
      if (!w) {
        this.#enter(e, spot, instant, now);
        continue;
      }
      w.employee = e;
      if (w.leaving) {
        w.leaving = false;
        if (w.fade) w.fade = { t0: now, from: this.#opacity(w, now), to: 1 };
      }
      // A walker on a detour goes on to its new place afterwards, unless
      // nothing may walk now.
      if (w.home !== null && !instant) {
        w.home = spot.id;
        continue;
      }
      w.home = null;
      if (w.goal !== spot.id) this.#setGoal(w, spot.id, instant, now);
    }
    this.#updateHelpers(layout.helpers, instant, now);
    this.#seen = live;
    this.#takeBriefings(layout, instant, now);
    this.#started = true;
    this.#afterChange(now);
  }

  /** Each briefing a layout names is taken once per work order: queued
   *  when a walk would be drawn (walking on, motion allowed, the window
   *  shown, a layout before this one), let go otherwise (D-568). */
  #takeBriefings(layout: SceneLayout, instant: boolean, now: number): void {
    for (const e of layout.employees) {
      const orderId = e.briefing?.orderId;
      if (orderId === undefined || this.#briefed.has(orderId)) continue;
      this.#briefed.add(orderId);
      if (!instant && this.#walking && this.#walkers.has(e.key)) this.#queue.push({ orderId, key: e.key });
    }
    this.#keepDetour(now);
  }

  /** Ends a detour whose employee or orchestrator is gone or back home,
   *  sending the other home; then starts the next briefing, if any. */
  #keepDetour(now: number): void {
    const d = this.#detour;
    if (d) {
      const pair = [this.#walkers.get(d.key), this.#walkers.get(ORCHESTRATOR_KEY)];
      if (pair.some((w) => !w || w.leaving || w.home === null)) {
        for (const w of pair) if (w && w.home !== null) this.#goHome(w, now);
        this.#detour = null;
      }
    }
    while (!this.#detour && this.#queue.length > 0) {
      const { key } = this.#queue.shift()!;
      const w = this.#walkers.get(key);
      const o = this.#walkers.get(ORCHESTRATOR_KEY);
      if (!w || w.leaving || !o || o.leaving) continue;
      this.#detour = { key, dwellUntil: null };
      for (const [walker, via] of [
        [w, briefingSpot().id],
        [o, staffSpots().counter.id],
      ] as const) {
        walker.home = walker.goal;
        this.#setGoal(walker, via, false, now);
      }
    }
  }

  /** A walker back from the counter to its place. */
  #goHome(w: Walker, now: number): void {
    const home = w.home!;
    w.home = null;
    this.#setGoal(w, home, false, now);
  }

  /** The dwell starts once both stand at the counter, and ends after
   *  `BRIEFING_DWELL_MS`, each going on to its place. */
  #stepDetour(now: number): void {
    const d = this.#detour;
    if (!d) return;
    const pair = [this.#walkers.get(d.key), this.#walkers.get(ORCHESTRATOR_KEY)];
    if (d.dwellUntil === null) {
      if (pair.every((w) => w && w.home !== null && !w.route && !w.glide && w.at === w.goal)) d.dwellUntil = now + BRIEFING_DWELL_MS;
      return;
    }
    if (now < d.dwellUntil) return;
    for (const w of pair) if (w && w.home !== null) this.#goHome(w, now);
    this.#detour = null;
    this.#keepDetour(now);
  }

  /** A new sub-agent fades in at its place; one gone from the layout holds,
   *  then fades out; one back during its hold or fade is the same figure,
   *  its fade turned back (D-548). */
  #updateHelpers(helpers: readonly Helper[], instant: boolean, now: number): void {
    const next = new Map(helpers.map((h) => [h.key, h]));
    for (const h of this.#helpers.values()) h.still = null;
    for (const [key, h] of this.#helpers) {
      if (next.has(key) || h.leaving) continue;
      if (instant) {
        this.#helpers.delete(key);
        continue;
      }
      h.leaving = true;
      h.fade = { t0: now + HELPER_HOLD_MS, from: this.#fadeOf(h.fade, now), to: 0 };
    }
    for (const helper of next.values()) {
      const h = this.#helpers.get(helper.key);
      if (!h) {
        this.#helpers.set(helper.key, { helper, fade: instant ? null : { t0: now, from: 0, to: 1 }, leaving: false, still: null });
        continue;
      }
      h.helper = helper;
      if (!h.leaving) continue;
      h.leaving = false;
      const opacity = this.#fadeOf(h.fade, now);
      h.fade = opacity < 1 ? { t0: now, from: opacity, to: 1 } : null;
    }
  }

  /** Every figure as drawn now, leaving ones included. */
  figures(): Map<string, FigureFrame> {
    const now = this.#env.now();
    const out = new Map<string, FigureFrame>();
    for (const w of this.#walkers.values()) {
      if (w.still) {
        out.set(w.key, w.still);
        continue;
      }
      const frame = this.#frame(w, now);
      if (!this.#busy(w, now)) w.still = frame;
      out.set(w.key, frame);
    }
    for (const h of this.#helpers.values()) {
      if (h.still) {
        out.set(h.helper.key, h.still);
        continue;
      }
      const frame = helperFrame(h.helper, this.#fadeOf(h.fade, now));
      if (!h.fade) h.still = frame;
      out.set(h.helper.key, frame);
    }
    return out;
  }

  #walker(e: Employee, node: string): Walker {
    const n = this.#graph!.nodes.get(node)!;
    const w: Walker = {
      key: e.key,
      employee: e,
      goal: node,
      gx: n.gx,
      gy: n.gy,
      at: node,
      route: null,
      nodes: [],
      s: 0,
      k: 0,
      v: 0,
      glide: null,
      prev: null,
      next: null,
      svx: 0,
      svy: 0,
      phase: 0,
      flip: false,
      view: "front",
      sitAt: null,
      fade: null,
      leaving: false,
      still: null,
      home: null,
    };
    this.#walkers.set(e.key, w);
    return w;
  }

  /** A key without a figure: placed at once, or faded in on the exit and
   *  walked in when it is new to the board (D-483), or faded in at its
   *  place when it comes from a group or walking is off. The orchestrator
   *  fades in at its chair: it does not come through the entrance. */
  #enter(e: Employee, spot: Spot, instant: boolean, now: number): void {
    if (instant) {
      this.#arrive(this.#walker(e, spot.id), now, true);
      return;
    }
    if (this.#walking && !this.#seen.has(e.key) && e.key !== ORCHESTRATOR_KEY) {
      const w = this.#walker(e, this.#graph!.exit);
      w.fade = { t0: now, from: 0, to: 1 };
      this.#setGoal(w, spot.id, false, now);
      return;
    }
    const w = this.#walker(e, spot.id);
    w.fade = { t0: now, from: 0, to: 1 };
    this.#arrive(w, now, true);
  }

  /** A figure whose key has no place any more: an ended session walks out
   *  and fades at the exit (D-483); one listed in a group, or any with
   *  walking off, fades where it stands. */
  #leave(w: Walker, live: boolean, instant: boolean, now: number): void {
    if (instant) {
      this.#walkers.delete(w.key);
      return;
    }
    if (w.leaving) return;
    this.#standUp(w, now);
    w.leaving = true;
    w.home = null;
    if (!live && this.#walking) {
      this.#setGoal(w, this.#graph!.exit, false, now);
      return;
    }
    w.route = null;
    w.glide = null;
    w.v = 0;
    w.fade = { t0: now, from: this.#opacity(w, now), to: 0 };
  }

  #setGoal(w: Walker, goal: string, instant: boolean, now: number): void {
    this.#standUp(w, now);
    w.goal = goal;
    w.sitAt = null;
    const target = this.#graph!.nodes.get(goal)!;
    if (instant) {
      this.#place(w, goal);
      this.#arrive(w, now, true);
      return;
    }
    if (!this.#walking) {
      w.route = null;
      w.glide = { x0: w.gx, y0: w.gy, x1: target.gx, y1: target.gy, t0: now };
      w.prev = w.at ?? w.prev;
      w.next = goal;
      w.at = null;
      this.#faceToward(w, target.gx - w.gx, target.gy - w.gy);
      return;
    }
    const start: RouteStart = w.at ?? { gx: w.gx, gy: w.gy, prev: w.prev, next: w.next };
    const ids = route(this.#graph!, start, goal);
    if (!ids) {
      this.#place(w, goal);
      this.#arrive(w, now, true);
      return;
    }
    const raw: { x: number; y: number; node: string | null }[] = [{ x: w.gx, y: w.gy, node: w.at }];
    for (const id of ids) {
      const n = this.#graph!.nodes.get(id)!;
      const last = raw[raw.length - 1]!;
      if (Math.hypot(n.gx - last.x, n.gy - last.y) < 1e-4) {
        last.node = id;
        continue;
      }
      raw.push({ x: n.gx, y: n.gy, node: id });
    }
    w.glide = null;
    if (raw.length < 2) {
      w.route = null;
      w.at = raw[0]!.node;
      this.#arrive(w, now, false);
      return;
    }
    w.route = smooth(raw);
    w.nodes = raw.map((p) => p.node);
    w.s = 0;
    w.k = 0;
    w.prev = raw[0]!.node ?? w.prev;
    w.next = raw[1]!.node;
    w.at = null;
  }

  /** A figure drawn into a seat leaves from where it is drawn, so it never
   *  jumps back to the seat's floor point: its walk starts there. */
  #standUp(w: Walker, now: number): void {
    if (w.route || w.glide || w.sitAt === null) return;
    const spot = this.#spots.get(w.goal);
    if (!spot?.sit) return;
    const u = this.#slide(w, now);
    w.gx += spot.sit.dx * u;
    w.gy += spot.sit.dy * u;
    w.sitAt = null;
  }

  /** How far into its seat a figure is drawn, 0 to 1. */
  #slide(w: Walker, now: number): number {
    return w.sitAt === null ? 0 : clamp01((now - w.sitAt + SLIDE_MS / 2) / SLIDE_MS);
  }

  #place(w: Walker, node: string): void {
    const n = this.#graph!.nodes.get(node)!;
    w.gx = n.gx;
    w.gy = n.gy;
    w.at = node;
    w.route = null;
    w.glide = null;
    w.v = 0;
  }

  /** At its goal: a leaving figure fades out; a seated place makes it sit,
   *  after a pause when it walked there, at once when it did not. */
  #arrive(w: Walker, now: number, instant: boolean): void {
    w.route = null;
    w.glide = null;
    w.v = 0;
    w.at = w.goal;
    if (w.leaving) {
      w.fade = { t0: now, from: this.#opacity(w, now), to: 0 };
      return;
    }
    // At the counter it stands, whatever its place's pose.
    if (w.employee.pose === "standing" || w.home !== null) {
      w.sitAt = null;
      return;
    }
    if (instant) w.sitAt = Number.NEGATIVE_INFINITY;
    else w.sitAt = now + (this.#walking ? WALK.sitPause * 1000 : SLIDE_MS / 2);
  }

  #faceToward(w: Walker, dgx: number, dgy: number): void {
    const vx = (dgx - dgy) * HW;
    const vy = (dgx + dgy) * HH;
    if (Math.abs(vx) > 1) w.flip = vx < 0;
    if (Math.abs(vy) > 1) w.view = vy > 0 ? "front" : "back";
  }

  #advance(w: Walker, dt: number, now: number): void {
    const r = w.route!;
    const total = r[r.length - 1]!.d;
    const remaining = total - w.s;
    const vStop = Math.sqrt(2 * WALK.dec * Math.max(0, remaining));
    w.v = Math.min(WALK.vmax, w.v + WALK.acc * dt, Math.max(WALK.vmin, vStop));
    const ds = Math.min(remaining, w.v * dt);
    w.s += ds;
    while (w.k < r.length - 2 && r[w.k + 1]!.d <= w.s) w.k++;
    const p = r[w.k]!;
    const q = r[w.k + 1]!;
    const t = clamp01((w.s - p.d) / Math.max(1e-6, q.d - p.d));
    const nx = p.x + (q.x - p.x) * t;
    const ny = p.y + (q.y - p.y) * t;
    if (dt > 0) {
      const vx = ((nx - w.gx - (ny - w.gy)) * HW) / dt;
      const vy = ((nx - w.gx + (ny - w.gy)) * HH) / dt;
      const a = 1 - Math.exp(-dt / WALK.facing.smoothing);
      w.svx += (vx - w.svx) * a;
      w.svy += (vy - w.svy) * a;
      const speed = Math.hypot(w.svx, w.svy);
      if (speed > WALK.facing.minSpeed) {
        if (w.svx > WALK.facing.horizontal * speed) w.flip = false;
        else if (w.svx < -WALK.facing.horizontal * speed) w.flip = true;
        if (w.svy > WALK.facing.vertical * speed) w.view = "front";
        else if (w.svy < -WALK.facing.vertical * speed) w.view = "back";
      }
    }
    w.gx = nx;
    w.gy = ny;
    w.at = null;
    w.phase += (ds / WALK.step) * Math.PI;
    w.prev = w.nodes[q.seg] ?? w.prev;
    w.next = w.nodes[q.seg + 1] ?? w.next;
    if (w.s >= total - 1e-6) {
      const last = r[r.length - 1]!;
      w.gx = last.x;
      w.gy = last.y;
      this.#arrive(w, now, false);
    }
  }

  #stepGlide(w: Walker, now: number): void {
    const g = w.glide!;
    const t = clamp01((now - g.t0) / GLIDE_MS);
    const k = easeInOut(t);
    w.gx = g.x0 + (g.x1 - g.x0) * k;
    w.gy = g.y0 + (g.y1 - g.y0) * k;
    if (t >= 1) {
      this.#place(w, w.goal);
      this.#arrive(w, now, false);
    }
  }

  #opacity(w: Walker, now: number): number {
    return this.#fadeOf(w.fade, now);
  }

  #fadeOf(fade: { t0: number; from: number; to: number } | null, now: number): number {
    if (!fade) return 1;
    const t = clamp01((now - fade.t0) / FADE_MS);
    return Math.round((fade.from + (fade.to - fade.from) * t) * 100) / 100;
  }

  #frame(w: Walker, now: number): FigureFrame {
    const e = w.employee;
    const moving = w.route !== null || w.glide !== null;
    const spot = w.leaving ? undefined : this.#spots.get(w.goal);
    let drawGx = w.gx;
    let drawGy = w.gy;
    let pose: Pose = "standing";
    let facing = w.view;
    let marks = walkingMarks(e.marks);
    if (!moving && spot && w.home !== null) {
      // At the counter: standing, facing across it, its marks kept for
      // its place.
      facing = spot.facing;
    } else if (!moving && spot) {
      const seated = w.sitAt !== null && now >= w.sitAt;
      if (spot.sit && w.sitAt !== null) {
        const u = this.#slide(w, now);
        drawGx += spot.sit.dx * u;
        drawGy += spot.sit.dy * u;
      }
      pose = seated ? e.pose : "standing";
      // Before it sits, it stands facing its seat as the place does.
      facing = seated || e.pose === "standing" ? e.facing : spot.facing;
      marks = e.marks;
    }
    return {
      key: w.key,
      project: e.project,
      gx: w.gx,
      gy: w.gy,
      drawGx,
      drawGy,
      moving,
      pose,
      facing,
      flip: w.flip,
      stride: w.route ? strideOf(w.phase) : null,
      opacity: this.#opacity(w, now),
      marks,
      alerted: e.alert !== null,
      helper: false,
      role: roleOf(w.key),
    };
  }

  /** Every walker at its place, seated if its place is a seat, every fade
   *  ended, the leaving ones gone (D-502). */
  #settleAll(): void {
    const now = this.#env.now();
    for (const w of [...this.#walkers.values()]) {
      if (w.leaving) {
        this.#walkers.delete(w.key);
        continue;
      }
      w.fade = null;
      w.still = null;
      if (w.home !== null) {
        w.goal = w.home;
        w.home = null;
      }
      if (w.route || w.glide || w.at !== w.goal) this.#place(w, w.goal);
      this.#arrive(w, now, true);
    }
    this.#detour = null;
    this.#queue = [];
    for (const [key, h] of this.#helpers) {
      if (h.leaving) this.#helpers.delete(key);
      h.fade = null;
      h.still = null;
    }
    if (this.#handle !== null) this.#env.cancel(this.#handle);
    this.#handle = null;
  }

  #busy(w: Walker, now: number): boolean {
    return w.route !== null || w.glide !== null || w.fade !== null || w.home !== null || (w.sitAt !== null && now < w.sitAt + SLIDE_MS / 2);
  }

  /** Starts or stops the loop, and tells the host and the halos. */
  #afterChange(now: number): void {
    const active = [...this.#walkers.values()].some((w) => this.#busy(w, now)) || [...this.#helpers.values()].some((h) => h.fade !== null);
    if (active && this.#handle === null) {
      this.#last = now;
      this.#handle = this.#env.request(this.#tick);
    } else if (!active && this.#handle !== null) {
      this.#env.cancel(this.#handle);
      this.#handle = null;
    }
    if (active !== this.#moving) {
      this.#moving = active;
      this.#host?.setAttribute("data-moving", active ? "1" : "0");
    }
    const transit = this.transit();
    const signature = [...transit].sort().join("\n");
    if (signature !== this.#transit) {
      this.#transit = signature;
      this.onTransit?.(transit);
    }
  }

  #notify(): void {
    if (this.#listeners.size === 0) return;
    const figures = this.figures();
    for (const listener of this.#listeners) listener(figures);
  }

  readonly #tick = (at: number): void => {
    const now = this.#env.now();
    if (at - this.#lastFrameAt < MIN_FRAME_MS) {
      this.#handle = this.#env.request(this.#tick);
      return;
    }
    this.#lastFrameAt = at;
    this.#handle = null;
    const dt = (now - this.#last) / 1000;
    this.#last = now;
    if (dt > FRAME_GAP_S) {
      this.#settleAll();
    } else {
      let left = Math.min(dt, FRAME_CAP_S);
      while (left > 1e-9) {
        const step = Math.min(SUBSTEP_S, left);
        for (const w of this.#walkers.values()) if (w.route) this.#advance(w, step, now);
        left -= step;
      }
      for (const w of this.#walkers.values()) if (w.glide) this.#stepGlide(w, now);
      this.#stepDetour(now);
      for (const w of [...this.#walkers.values()]) {
        if (!w.fade || now - w.fade.t0 < FADE_MS) continue;
        if (w.fade.to === 0) this.#walkers.delete(w.key);
        else w.fade = null;
      }
      for (const [key, h] of [...this.#helpers]) {
        if (!h.fade || now - h.fade.t0 < FADE_MS) continue;
        if (h.fade.to === 0) this.#helpers.delete(key);
        else h.fade = null;
        h.still = null;
      }
    }
    this.#notify();
    this.#afterChange(now);
  };
}
