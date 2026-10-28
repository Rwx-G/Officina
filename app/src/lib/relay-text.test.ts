import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { activityRow } from "./relay-text";
import RecentActivity from "../components/RecentActivity.svelte";
import type { RecentEntry } from "./generated/RecentEntry";
import type { RunFinished } from "./generated/RunFinished";
import type { Tone } from "./generated/Tone";

const NOW = 1_800_000_000_000;
const HOSTILE = `<img src=x onerror="window.__hit = 1"><b>bold</b>`;
const stream = { bytes: 0, sha256: "e3b0" };

function run(
  state: "running" | "finished" | "abandoned",
  finished: Partial<RunFinished> | null,
  tone: Tone | null,
): RecentEntry {
  return {
    ts: NOW - 60_000,
    tone,
    event: {
      kind: "run",
      state,
      started: {
        run_id: "r1",
        project: "lab",
        session_id: null,
        action: "probe",
        class: "free",
        params: { host: "web01" },
        argv: ["C:/path/to/tool.exe", "web01"],
        catalogue_hash: "00",
        timeout_s: 60,
      },
      finished: finished && {
        run_id: "r1",
        outcome: "exited",
        exit_code: 0,
        duration_ms: 350,
        stdout: stream,
        stderr: stream,
        ...finished,
      },
      finished_at_ms: finished ? NOW : null,
    },
  };
}

afterEach(() => cleanup());

describe("activityRow", () => {
  it("states a run's exit, its duration and the tone Rust gave it", () => {
    expect(activityRow(run("finished", {}, "ok"))).toMatchObject({
      project: "lab",
      action: "probe",
      params: [["host", "web01"]],
      outcome: "exit 0",
      durationMs: 350,
      tone: "ok",
    });
    expect(activityRow(run("finished", { exit_code: 3 }, "failed"))).toMatchObject({ outcome: "exit 3", tone: "fault" });
    expect(activityRow(run("finished", { outcome: "timeout", exit_code: null }, "failed"))).toMatchObject({
      outcome: "timed out",
      tone: "fault",
    });
  });

  it("states an HTTP run's status, or that no response came", () => {
    expect(activityRow(run("finished", { outcome: "http", exit_code: null, status: 200 }, "ok"))).toMatchObject({
      outcome: "http 200",
      tone: "ok",
    });
    expect(activityRow(run("finished", { outcome: "http", exit_code: null, status: 503 }, "failed"))).toMatchObject({
      outcome: "http 503",
      tone: "fault",
    });
    expect(
      activityRow(run("finished", { outcome: "connect_failed", exit_code: null, detail: "refused" }, "failed")),
    ).toMatchObject({ outcome: "no response", detail: "refused", tone: "fault" });
  });

  it("marks a run without an end line as running or abandoned", () => {
    expect(activityRow(run("running", null, null))).toMatchObject({ outcome: "running", durationMs: null, tone: "neutral" });
    expect(activityRow(run("abandoned", null, "failed"))).toMatchObject({ outcome: "abandoned", tone: "abandoned" });
  });

  it("names a refusal, a gate deny and a gate error with what came with them", () => {
    expect(
      activityRow({
        ts: NOW,
        tone: "refused",
        event: {
          kind: "refused",
          project: "lab",
          session_id: null,
          action: "apply",
          params: { group: "lab-db" },
          reason: "check_required",
          detail: "run `check` first",
        },
      }),
    ).toMatchObject({ outcome: "refused: its check has not run", detail: "run `check` first", tone: "fault" });
    expect(
      activityRow({
        ts: NOW,
        tone: "refused",
        event: { kind: "gate_denied", project: "lab", session_id: null, deny: "guarded_program", matched: "ssh", command_excerpt: "ssh host01" },
      }),
    ).toMatchObject({ action: "gate", outcome: "denied: guarded program ssh", detail: "ssh host01", tone: "fault" });
    expect(
      activityRow({ ts: NOW, tone: "failed", event: { kind: "gate_error", session_id: null, cause: "floor_invalid", detail: "line 1" } }),
    ).toMatchObject({ outcome: "gate error: relay floor invalid", detail: "line 1", tone: "fault" });
  });
});

describe("RecentActivity", () => {
  it("renders session text as text, never as markup", () => {
    const { container } = render(RecentActivity, {
      props: {
        nowMs: NOW,
        recent: [
          {
            ts: NOW,
            tone: "refused",
            event: { kind: "gate_denied", project: "lab", session_id: null, deny: "dynamic", command_excerpt: HOSTILE },
          },
          {
            ts: NOW,
            tone: "refused",
            event: {
              kind: "refused",
              session_id: null,
              action: "probe",
              params: { host: HOSTILE },
              reason: "invalid_value",
              detail: HOSTILE,
            },
          },
        ],
      },
    });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    expect(container.textContent).toContain(HOSTILE);
    expect(container.querySelectorAll("[data-testid=recent-entry]")).toHaveLength(2);
  });
});
