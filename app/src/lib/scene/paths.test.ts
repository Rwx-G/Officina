// Routes and smoothing (AD-170): routes on graph edges, the start of a
// route from mid-walk, rounded corners within their radius, determinism.
import { describe, expect, it } from "vitest";
import { MAX_COLUMNS } from "./geometry";
import { CORNER_RADIUS, route, smooth, type PathPoint } from "./paths";
import { allSpots, navGraph, type NavGraph } from "./places";

const graph = navGraph(MAX_COLUMNS);
const spots = allSpots(MAX_COLUMNS);

function length(g: NavGraph, ids: string[]): number {
  let total = 0;
  for (let i = 1; i < ids.length; i++) {
    const a = g.nodes.get(ids[i - 1]!)!;
    const b = g.nodes.get(ids[i]!)!;
    total += Math.hypot(b.gx - a.gx, b.gy - a.gy);
  }
  return total;
}

/** The distance from `p` to the segment `a`-`b`. */
function toSegment(p: PathPoint, a: PathPoint, b: PathPoint): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy || 1)));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

/** A spot pair of every kind: across the floor, within a room, to itself. */
const PAIRS: [string, string][] = spots.flatMap((s, i) => [
  [s.id, spots[(i * 7 + 3) % spots.length]!.id],
  [s.id, s.id],
]);

describe("paths", () => {
  it("a route follows graph edges", () => {
    for (const [from, to] of PAIRS) {
      const ids = route(graph, from, to)!;
      expect(ids, `${from} to ${to}`).not.toBeNull();
      expect(ids[0]).toBe(from);
      expect(ids[ids.length - 1]).toBe(to);
      for (let i = 1; i < ids.length; i++) expect(graph.nodes.get(ids[i - 1]!)!.adj, `${from} to ${to}`).toContain(ids[i]);
      expect(new Set(ids).size, `${from} to ${to} visits a node twice`).toBe(ids.length);
      // No route through one neighbour of the start is shorter.
      const best = length(graph, ids);
      for (const next of graph.nodes.get(from)!.adj) {
        const via = route(graph, next, to)!;
        const a = graph.nodes.get(from)!;
        const b = graph.nodes.get(next)!;
        expect(best, `${from} to ${to} via ${next}`).toBeLessThanOrEqual(Math.hypot(b.gx - a.gx, b.gy - a.gy) + length(graph, via) + 1e-9);
      }
    }
  });

  it("a route from mid-walk starts at the nearer node", () => {
    // A walker a fifth of the way along every edge of the corridor's
    // chain, heading for each end of the floor.
    const exit = graph.exit;
    const far = spots[spots.length - 1]!.id;
    for (const node of graph.nodes.values()) {
      for (const next of node.adj) {
        const a = node;
        const b = graph.nodes.get(next)!;
        const at = { gx: a.gx + (b.gx - a.gx) * 0.2, gy: a.gy + (b.gy - a.gy) * 0.2, prev: a.id, next: b.id };
        for (const goal of [exit, far]) {
          const ids = route(graph, at, goal)!;
          expect([a.id, b.id], `${a.id} - ${b.id} to ${goal}`).toContain(ids[0]);
          // The start chosen is the nearer one along the way.
          const total = (id: string) => {
            const n = graph.nodes.get(id)!;
            return Math.hypot(n.gx - at.gx, n.gy - at.gy) + length(graph, route(graph, id, goal)!);
          };
          expect(total(ids[0]!)).toBeLessThanOrEqual(Math.min(total(a.id), total(b.id)) + 1e-9);
        }
      }
    }
    // With no node to start from there is no route.
    expect(route(graph, { gx: 0, gy: 0, prev: null, next: null }, exit)).toBeNull();
  });

  it("smoothing keeps both ends and stays within the corner radius", () => {
    // The mover smooths a walk of two points or more.
    const routes: PathPoint[][] = PAIRS.filter(([from, to]) => from !== to).map(([from, to]) =>
      route(graph, from, to)!.map((id) => ({ x: graph.nodes.get(id)!.gx, y: graph.nodes.get(id)!.gy })),
    );
    // Short segments: a corner takes at most half of each.
    routes.push([{ x: 0, y: 0 }, { x: 0.3, y: 0 }, { x: 0.3, y: 0.2 }, { x: 1, y: 0.2 }]);
    for (const raw of routes) {
      const out = smooth(raw);
      expect(out[0]).toMatchObject(raw[0]!);
      expect(out[out.length - 1]).toMatchObject(raw[raw.length - 1]!);
      out.forEach((p, i) => {
        if (i > 0) expect(p.d).toBeGreaterThanOrEqual(out[i - 1]!.d);
        // On a raw segment, or within its corner's radius of the corner.
        const onSegment = raw.slice(1).some((b, j) => toSegment(p, raw[j]!, b) < 1e-9);
        const nearCorner = raw.slice(1, -1).some((b, j) => {
          const a = raw[j]!;
          const c = raw[j + 2]!;
          const r = Math.min(CORNER_RADIUS, Math.hypot(b.x - a.x, b.y - a.y) / 2, Math.hypot(c.x - b.x, c.y - b.y) / 2);
          return Math.hypot(p.x - b.x, p.y - b.y) <= r + 1e-9;
        });
        expect(onSegment || nearCorner, `${JSON.stringify(p)} of ${JSON.stringify(raw)}`).toBe(true);
      });
    }
  });

  it("the same input gives the same route", () => {
    for (const [from, to] of PAIRS.slice(0, 20)) {
      expect(route(navGraph(MAX_COLUMNS), from, to)).toEqual(route(graph, from, to));
      const raw = route(graph, from, to)!.map((id) => ({ x: graph.nodes.get(id)!.gx, y: graph.nodes.get(id)!.gy }));
      expect(smooth(raw)).toEqual(smooth(structuredClone(raw)));
    }
  });
});
