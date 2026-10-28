import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import ApprovalCard from "./ApprovalCard.svelte";
import Gauge from "./Gauge.svelte";
import SessionCard from "./SessionCard.svelte";
import { BoardStore } from "../lib/board.svelte";
import type { ApprovalView } from "../lib/generated/ApprovalView";
import type { Card } from "../lib/generated/Card";

vi.mock("../lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/ipc")>()),
  decideApproval: vi.fn(async () => undefined),
}));

const NOW = 1_800_000_000_000;

afterEach(cleanup);

function card(overrides: Partial<Card> = {}): Card {
  return {
    key: "pid:1:1",
    column: "needs_you",
    badge: null,
    kind: "interactive",
    name: "billing-api",
    project: { name: "billing-api", color: "blue" },
    session_id: "s",
    background_id: null,
    pid: 1,
    cwd: "C:/work/billing-api",
    waiting_for: "permission prompt",
    last_prompt: null,
    last_message: null,
    last_event_at_ms: NOW - 4 * 60_000,
    started_at_ms: NOW - 60 * 60_000,
    attach_available: false,
    ...overrides,
  };
}

function store(): BoardStore {
  const s = new BoardStore();
  s.nowMs = NOW;
  return s;
}

describe("SessionCard", () => {
  it("shows name, waiting reason and times", () => {
    const { getByText, getByTestId } = render(SessionCard, { card: card(), store: store() });
    expect(getByText("billing-api", { selector: ".name" })).toBeTruthy();
    expect(getByTestId("waiting-for").textContent).toBe("permission prompt");
    expect(getByText("4 min ago")).toBeTruthy();
    expect(getByText("running 1 h")).toBeTruthy();
  });

  it("renders session text as text, never as markup", () => {
    const hostile = `<img src=x onerror="alert(1)"><b>x</b>`;
    const { container, getByTestId } = render(SessionCard, {
      card: card({ last_prompt: hostile, last_message: hostile, name: hostile }),
      store: store(),
    });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    expect(getByTestId("last-prompt").textContent).toBe(hostile);
  });

  it("numbers Needs you cards for the keyboard and offers attach on background cards", () => {
    const { getByText, queryByText } = render(SessionCard, {
      card: card({ kind: "background", background_id: "abc", attach_available: true }),
      store: store(),
      index: 0,
    });
    expect(getByText("1")).toBeTruthy();
    expect(getByText("Copy attach command")).toBeTruthy();
    expect(queryByText("Wrong window?")).toBeNull();
  });

  it("shows the badge of an unknown state", () => {
    const { getByTestId } = render(SessionCard, {
      card: card({ column: "working", waiting_for: null, badge: "unknown" }),
      store: store(),
    });
    expect(getByTestId("badge").textContent).toBe("unknown");
  });
});

describe("Gauge", () => {
  const base = { label: "5 h", weekly: false, staleAfterMs: 30 * 60_000, nowMs: NOW };

  it("says there is no data instead of showing zero", () => {
    const { getByText } = render(Gauge, { ...base, window: null, writtenAtMs: null });
    expect(getByText("--")).toBeTruthy();
    expect(getByText("no data: see officina doctor")).toBeTruthy();
  });

  it("flags stale data with its age", () => {
    const { getByText } = render(Gauge, {
      ...base,
      window: { used_percentage: 62.4, resets_at: NOW / 1000 + 3600 },
      writtenAtMs: NOW - 45 * 60_000,
    });
    expect(getByText("62 %")).toBeTruthy();
    expect(getByText("stale, updated 45 min ago")).toBeTruthy();
  });

  it("never shows a percentage once the window has reset", () => {
    const { getByText } = render(Gauge, {
      ...base,
      window: { used_percentage: 90, resets_at: NOW / 1000 - 60 },
      writtenAtMs: NOW - 5 * 60_000,
    });
    expect(getByText("--")).toBeTruthy();
    expect(getByText("reset, no new data")).toBeTruthy();
  });
});

/** What a card gets besides its approval: its own state and decision. */
const onDecide = vi.fn();
function decision() {
  return { busy: false, error: undefined, nowMs: NOW, onDecide };
}

function approval(overrides: Partial<ApprovalView> = {}): ApprovalView {
  return {
    id: "0123456789abcdef-1",
    project: "infra",
    action: "restart",
    description: "Restart one service",
    argv: ["C:/path/to/tool.exe", "restart", "web01"],
    http: null,
    valid: true,
    invalid_reason: null,
    session_id: "s",
    session_name: "deploy-lab",
    requested_at_ms: NOW - 3 * 60_000,
    expires_at_ms: NOW + 57 * 60_000,
    dry_run: null,
    binding: "not_git_tree",
    bound_files: null,
    files_unchanged: null,
    ...overrides,
  };
}

describe("ApprovalCard", () => {
  beforeEach(() => onDecide.mockClear());

  it("renders one chip per argument", () => {
    const argv = ["C:/path/to/tool.exe", "--limit", "lab web", "playbooks/site.yml"];
    const { getAllByTestId, getByTestId, getByText } = render(ApprovalCard, {
      approval: approval({ argv }),
      ...decision(),
    });
    expect(getAllByTestId("arg").map((chip) => chip.textContent)).toEqual(argv);
    expect(getByTestId("approval-session").textContent).toBe("deploy-lab");
    expect(getByText("asked 3 min ago")).toBeTruthy();
    expect(getByText("expires in 57 min")).toBeTruthy();
    cleanup();
    const unknown = render(ApprovalCard, { approval: approval({ session_name: null }), ...decision() });
    expect(unknown.getByTestId("approval-session").textContent).toBe("session unknown");
  });

  it("bidi and markup in arguments stay inert text", () => {
    const argv = [`<img src=x onerror="window.__hit = 1">`, "\u202Egnp.exe", "a b"];
    const { container, getAllByTestId } = render(ApprovalCard, {
      approval: approval({ argv, description: "<b>bold</b>" }),
      ...decision(),
    });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    const chips = getAllByTestId("arg");
    expect(chips.map((chip) => chip.textContent)).toEqual(argv);
    expect(chips.every((chip) => chip.children.length === 0)).toBe(true);
  });

  it("an HTTP request shows its method, URL and credential as inert text", () => {
    const url = `https://monitoring.example.com/api/<img src=x onerror="window.__hit = 1">`;
    const { container, getByTestId, queryByTestId } = render(ApprovalCard, {
      approval: approval({ argv: [], http: { method: "GET", url, credential: "monitoring" } }),
      ...decision(),
    });
    expect(getByTestId("http-method").textContent).toBe("GET");
    expect(getByTestId("http-url").textContent).toBe(url);
    expect(getByTestId("http-credential").textContent).toBe("credential monitoring");
    expect(queryByTestId("argv")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
  });

  it("a pointer click calls decideApproval with the card id", async () => {
    const { getByTestId } = render(ApprovalCard, { approval: approval(), ...decision() });
    await fireEvent.click(getByTestId("approve"), { detail: 1 });
    expect(onDecide).toHaveBeenCalledWith(true);
    await fireEvent.click(getByTestId("reject"), { detail: 1 });
    expect(onDecide).toHaveBeenLastCalledWith(false);
  });

  it("Enter and Space on a focused Approve button do not decide", async () => {
    const { getByTestId } = render(ApprovalCard, { approval: approval(), ...decision() });
    for (const button of [getByTestId("approve"), getByTestId("reject")]) {
      button.focus();
      for (const key of ["Enter", " "]) {
        const keydown = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
        button.dispatchEvent(keydown);
        expect(keydown.defaultPrevented).toBe(true);
      }
      // What a browser does for a key activation: a click with detail 0.
      await fireEvent.click(button, { detail: 0 });
    }
    expect(onDecide).not.toHaveBeenCalled();
  });

  it("an invalid request shows its reason and no button", () => {
    const { queryByTestId, getByTestId } = render(ApprovalCard, {
      approval: approval({ valid: false, invalid_reason: "its key does not match", argv: [] }),
      ...decision(),
    });
    expect(getByTestId("invalid-reason").textContent).toContain("its key does not match");
    expect(queryByTestId("approve")).toBeNull();
    expect(queryByTestId("reject")).toBeNull();
    expect(queryByTestId("argv")).toBeNull();
  });

  it("ApprovalCard shows the dry run as inert text", () => {
    const output = `TASK [restart] <script>window.__hit = 1</script>\nchanged: [web01]\n`;
    const { container, getByTestId, rerender } = render(ApprovalCard, {
      approval: approval({ dry_run: { check: "check", run_id: "r1", output, missing: null } }),
      ...decision(),
    });
    expect(container.querySelector("script")).toBeNull();
    expect(getByTestId("dry-run-output").textContent).toBe(output);
    rerender({
      approval: approval({ dry_run: { check: "check", run_id: "r1", output: null, missing: "not kept" } }),
      ...decision(),
    });
    expect(getByTestId("dry-run-missing").textContent).toBe("not kept");
  });

  // D-170, D-173: the card says what the files are bound to, never "tree
  // unchanged": since the dry run, since the request, or not at all.
  it("ApprovalCard states its binding", () => {
    const line = (overrides: Partial<ApprovalView>): { text: string; changed: boolean } => {
      const { getByTestId } = render(ApprovalCard, { approval: approval(overrides), ...decision() });
      const element = getByTestId("binding");
      const result = { text: element.firstChild?.textContent?.trim() ?? "", changed: element.classList.contains("changed") };
      cleanup();
      return result;
    };
    const dryRun = { check: "check", run_id: "r1", output: "ok", missing: null };
    expect(line({ binding: "dry_run", dry_run: dryRun, bound_files: 12, files_unchanged: true })).toEqual({
      text: "tracked and unignored files unchanged since the dry run",
      changed: false,
    });
    expect(line({ binding: "request", bound_files: 12, files_unchanged: true })).toEqual({
      text: "tracked and unignored files unchanged since the request",
      changed: false,
    });
    expect(line({ binding: "request", bound_files: 12, files_unchanged: false })).toEqual({
      text: "tracked or unignored files changed since the request: the run will refuse this approval",
      changed: true,
    });
    expect(line({ binding: "request", bound_files: 12, files_unchanged: null }).text).toBe(
      "tracked and unignored files bound to the request",
    );
    expect(line({ binding: "not_git_tree" })).toEqual({ text: "no file binding: not a git tree", changed: false });
    const counted = render(ApprovalCard, {
      approval: approval({ binding: "request", bound_files: 12, files_unchanged: true }),
      ...decision(),
    });
    expect(counted.getByTestId("bound-files").textContent?.trim()).toBe("(12 files)");
    cleanup();
    const invalid = render(ApprovalCard, {
      approval: approval({ valid: false, invalid_reason: "forged", argv: [], binding: null }),
      ...decision(),
    });
    expect(invalid.queryByTestId("binding")).toBeNull();
  });
});
