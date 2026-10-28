// Routes on the navigation graph and their rounded corners (AD-170), ported
// from the prototype v2's `shortestPath`, `planRoute` and `smooth`. Pure
// and deterministic: the same graph and input give the same route.
import type { NavGraph } from "./places";

/** The radius of a route's rounded corners, in tiles; a corner never takes
 *  more than half of either segment it joins. */
export const CORNER_RADIUS = 0.45;
/** Quadratic samples per rounded corner. */
const CORNER_STEPS = 8;
/** A corner tighter than this radius stays sharp, as does a straight one. */
const MIN_RADIUS = 0.03;
const STRAIGHT_CROSS = 0.04;

/** Where a walker starts: a node it stands on, or a point between the
 *  nodes it walked from and toward. */
export type RouteStart = string | { gx: number; gy: number; prev: string | null; next: string | null };

/** The shortest path from `from` to `to` (Dijkstra on Euclidean edge
 *  lengths, the first node found on a tie), node ids from `from` to `to`;
 *  `null` when `to` cannot be reached. */
function shortestPath(graph: NavGraph, from: string, to: string): { ids: string[]; length: number } | null {
  if (from === to) return { ids: [from], length: 0 };
  const dist = new Map([[from, 0]]);
  const prev = new Map<string, string>();
  const open = new Set([from]);
  const done = new Set<string>();
  while (open.size > 0) {
    let u: string | null = null;
    for (const id of open) if (u === null || dist.get(id)! < dist.get(u)!) u = id;
    open.delete(u!);
    done.add(u!);
    if (u === to) break;
    const nu = graph.nodes.get(u!)!;
    for (const v of nu.adj) {
      if (done.has(v)) continue;
      const nv = graph.nodes.get(v)!;
      const alt = dist.get(u!)! + Math.hypot(nv.gx - nu.gx, nv.gy - nu.gy);
      if (!dist.has(v) || alt < dist.get(v)!) {
        dist.set(v, alt);
        prev.set(v, u!);
        open.add(v);
      }
    }
  }
  if (!dist.has(to)) return null;
  const ids = [to];
  while (ids[0] !== from) ids.unshift(prev.get(ids[0]!)!);
  return { ids, length: dist.get(to)! };
}

/** The nodes a walker at `from` follows to the node `to`. A walker between
 *  two nodes starts from the nearer of them along the way, its distance
 *  to the node counted with the route from it, so it turns back rather
 *  than walk on and return (the prototype's `planRoute`). */
export function route(graph: NavGraph, from: RouteStart, to: string): string[] | null {
  if (typeof from === "string") return shortestPath(graph, from, to)?.ids ?? null;
  let best: { ids: string[]; length: number } | null = null;
  for (const id of [from.next, from.prev]) {
    if (id === null) continue;
    const node = graph.nodes.get(id);
    if (!node) continue;
    const path = shortestPath(graph, id, to);
    if (!path) continue;
    const length = Math.hypot(node.gx - from.gx, node.gy - from.gy) + path.length;
    if (!best || length < best.length) best = { ids: path.ids, length };
  }
  return best?.ids ?? null;
}

export interface PathPoint {
  x: number;
  y: number;
}

/** A point of a smoothed route: `seg` is the index of the raw segment it
 *  lies on, `d` its distance along the route from the start. */
export interface RoutePoint extends PathPoint {
  seg: number;
  d: number;
}

/** `points` with each interior corner replaced by a quadratic curve of
 *  radius `CORNER_RADIUS`, at most half of each segment it joins; both
 *  ends are kept. */
export function smooth(points: readonly PathPoint[]): RoutePoint[] {
  const first = points[0]!;
  const out: Omit<RoutePoint, "d">[] = [{ x: first.x, y: first.y, seg: 0 }];
  for (let i = 1; i < points.length - 1; i++) {
    const a = points[i - 1]!;
    const b = points[i]!;
    const c = points[i + 1]!;
    const l1 = Math.hypot(b.x - a.x, b.y - a.y);
    const l2 = Math.hypot(c.x - b.x, c.y - b.y);
    const r = Math.min(CORNER_RADIUS, l1 / 2, l2 / 2);
    const u1 = [(b.x - a.x) / l1, (b.y - a.y) / l1] as const;
    const u2 = [(c.x - b.x) / l2, (c.y - b.y) / l2] as const;
    const cross = Math.abs(u1[0] * u2[1] - u1[1] * u2[0]);
    const dot = u1[0] * u2[0] + u1[1] * u2[1];
    if (r < MIN_RADIUS || (cross < STRAIGHT_CROSS && dot > 0)) {
      out.push({ x: b.x, y: b.y, seg: i });
      continue;
    }
    const A = [b.x - u1[0] * r, b.y - u1[1] * r] as const;
    const B = [b.x + u2[0] * r, b.y + u2[1] * r] as const;
    out.push({ x: A[0], y: A[1], seg: i - 1 });
    for (let k = 1; k <= CORNER_STEPS; k++) {
      const t = k / CORNER_STEPS;
      const m0 = (1 - t) * (1 - t);
      const m1 = 2 * (1 - t) * t;
      const m2 = t * t;
      out.push({ x: m0 * A[0] + m1 * b.x + m2 * B[0], y: m0 * A[1] + m1 * b.y + m2 * B[1], seg: t < 0.5 ? i - 1 : i });
    }
  }
  const last = points[points.length - 1]!;
  out.push({ x: last.x, y: last.y, seg: Math.max(0, points.length - 2) });
  let d = 0;
  return out.map((p, i) => {
    if (i > 0) d += Math.hypot(p.x - out[i - 1]!.x, p.y - out[i - 1]!.y);
    return { ...p, d };
  });
}
