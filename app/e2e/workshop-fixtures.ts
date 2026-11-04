// The workshop card's facts and the review views the window tests use
// (story 6.19, D-1236). A real review needs a package a worker committed
// and the host collected, which no fixture run has, so the views are
// typed literals checked by `pnpm check` against the generated types,
// while core's tests own how a view is built. The card under the facts is
// the real `card-bump` card. No Playwright import: Vitest loads this too.
import type { ReviewView } from "../src/lib/generated/ReviewView";
import type { WorkOrderCard } from "../src/lib/generated/WorkOrderCard";
import type { WorkshopCardFacts } from "../src/lib/generated/WorkshopCardFacts";

export const RUN_ID = "00000000000000000000000000006184";
const TIP = "1".repeat(40);
const BASE = "0".repeat(40);

export function workshopFacts(overrides: Partial<WorkshopCardFacts> = {}): WorkshopCardFacts {
  return {
    bases: [
      { choice: { listed: { branch: "main" } }, tip: BASE },
      { choice: { listed: { branch: "release" } }, tip: null },
      { choice: { imported: { branch: "officina/dependency-bump-3", parent_run_id: RUN_ID } }, tip: TIP },
    ],
    base: { listed: { branch: "main" } },
    uncommitted: 2,
    mounts: [{ path: "vendor/cache", state: "ready" }],
    allow: [{ host: "index.crates.io", route: null }],
    image: { tag: "officina-workshop:local", short_id: "0123456789ab", built_at_ms: null },
    max_minutes: 30,
    max_usd_cents: 250,
    credential_kind: "oauth-token",
    readiness: [],
    plan_sha256: "2".repeat(64),
    ...overrides,
  };
}

export function workshopCard(base: WorkOrderCard, overrides: Partial<WorkshopCardFacts> = {}): WorkOrderCard {
  return { ...base, workshop: workshopFacts(overrides) };
}

/** A package in review: one commit, a diff, one sensitive group. */
export function review(overrides: Partial<ReviewView> = {}): ReviewView {
  const hunks = "@@ -1 +1 @@\n-on: push\n+on: [push, pull_request]\n";
  return {
    run_id: RUN_ID,
    template: "dependency-bump",
    project: "billing-api",
    base_branch: "main",
    base_commit: BASE,
    tip: TIP,
    state: "review",
    outcome: "package",
    cause: "exited",
    quarantine_cause: null,
    nothing_to_review: false,
    report_tail: "Bumped serde_json, the tests pass.",
    facts: {
      duration_ms: 5 * 60_000,
      cost_usd_micros: 420_000,
      turns: 12,
      exit_code: 0,
      hooks: {},
      allowed: [["index.crates.io", 14]],
      refused: 1,
      refusals: [{ host: "example.com", port: 443, cause: "not_allowed" }],
      clock_skew_ms: null,
      commits_past_base: 1,
      files_changed: 2,
      insertions: 4,
      deletions: 1,
      collect_failed: null,
      credential_locations: null,
      credential_places: [],
    },
    commits: [{ hash: TIP, subject: "Bump serde_json", author: "worker", at_ms: 1_800_000_000_000 }],
    files: [
      { index: 0, status: "M", paths: ["Cargo.lock"], groups: [] },
      { index: 1, status: "M", paths: [".github/workflows/ci.yml"], groups: ["ci"] },
    ],
    diff: {
      text: `diff --git a/.github/workflows/ci.yml b/.github/workflows/ci.yml\n${hunks}`,
      files: [{ path: ".github/workflows/ci.yml", line: 0, hunks: [1] }],
      truncated: false,
    },
    groups: [
      {
        key: "ci",
        group: "ci",
        files: [1],
        hunks,
        hunks_bytes: hunks.length,
        hunks_sha256: "3".repeat(64),
        approvable: true,
        approved: false,
      },
    ],
    readiness: { ready: false, reasons: [{ cause: "unapproved", group: "ci" }] },
    ...overrides,
  };
}

/** The same package with its group approved: Import is enabled. */
export function approvedReview(): ReviewView {
  const view = review();
  return {
    ...view,
    groups: view.groups.map((g) => ({ ...g, approved: true })),
    readiness: { ready: true, reasons: [] },
  };
}

/** A quarantined package. */
export function quarantinedReview(cause: ReviewView["quarantine_cause"] = "credential_found"): ReviewView {
  const view = review({ state: "quarantined", quarantine_cause: cause });
  return {
    ...view,
    facts: { ...view.facts, credential_locations: 1, credential_places: ["Cargo.lock"] },
    readiness: { ready: false, reasons: [{ cause: "not_in_review", group: null }] },
  };
}

/** A run whose work branch has no commit past its base. */
export function noChangeReview(): ReviewView {
  return review({
    state: "no_change",
    outcome: "no_change",
    nothing_to_review: true,
    tip: BASE,
    commits: [],
    files: [],
    diff: null,
    groups: [],
    report_tail: "Nothing to bump: serde_json is current.",
    readiness: { ready: false, reasons: [{ cause: "not_in_review", group: null }] },
  });
}

/** Every worker text of a review, each as a way to put `text` there: a
 *  test iterates these, so a field added here is a field it checks. */
export const UNTRUSTED_SOURCES: Record<string, (view: ReviewView, text: string) => ReviewView> = {
  report: (v, text) => ({ ...v, report_tail: text }),
  commit_subject: (v, text) => ({ ...v, commits: v.commits.map((c) => ({ ...c, subject: text })) }),
  commit_author: (v, text) => ({ ...v, commits: v.commits.map((c) => ({ ...c, author: text })) }),
  path: (v, text) => ({ ...v, files: v.files.map((f) => ({ ...f, paths: [text] })) }),
  diff: (v, text) => ({ ...v, diff: v.diff && { ...v.diff, text, files: v.diff.files.map((f) => ({ ...f, path: text })) } }),
  hunks: (v, text) => ({ ...v, groups: v.groups.map((g) => ({ ...g, hunks: text })) }),
  allowed_host: (v, text) => ({ ...v, facts: { ...v.facts, allowed: [[text, 1]] } }),
  refused_host: (v, text) => ({
    ...v,
    facts: { ...v.facts, refusals: v.facts.refusals.map((r) => ({ ...r, host: text })) },
  }),
};
