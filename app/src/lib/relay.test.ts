import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { fieldLines, isPointerClick, relayInUse, RelayStore, shown } from "./relay.svelte";
import RelayBanners from "../components/RelayBanners.svelte";
import type { RelayView } from "./generated/RelayView";
import type { CanonicalCatalogue } from "./generated/CanonicalCatalogue";

vi.mock("./ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./ipc")>()),
  approveCatalogue: vi.fn(async (hash: string) => hash),
  setRelayStopped: vi.fn(async () => true),
  getRelay: vi.fn(async () => null),
  onRelay: vi.fn(async () => () => {}),
}));

const ipc = await import("./ipc");
const NOW = 1_800_000_000_000;
const HOSTILE = `<img src=x onerror="window.__hit = 1"><b>bold</b>`;

function catalogue(description: string): CanonicalCatalogue {
  return {
    format: 1,
    floor_path: "c:/path/to/officina.toml",
    guarded_programs: ["ssh"],
    projects: { lab: { path: "c:/work/lab", autonomy: "guarded", actions: ["restart"] } },
    actions: {
      restart: {
        description,
        class: "free",
        runner: "native",
        distro: null,
        program: "C:/path/to/tool.exe",
        env: {},
        args: ["--service", "{service}"],
        params: { service: { values: ["web01"] } },
        check: null,
        timeout_s: 540,
        max_output_kib: 64,
      },
    },
    credentials: {},
  };
}

function view(overrides: Partial<RelayView> = {}): RelayView {
  return {
    schema: 1,
    stopped: false,
    stopped_since_ms: null,
    floor: "C:/path/to/officina.toml",
    floor_error: null,
    approved_hash: null,
    approved_at_ms: null,
    snapshot_error: null,
    catalogue: null,
    pending: {
      hash: "aa11",
      diff: {
        format: null,
        floor_path: null,
        guarded_programs: { added: ["ssh"], removed: [] },
        projects: { added: ["lab"], removed: [], changed: [] },
        actions: { added: ["restart"], removed: [], changed: [] },
        credentials: { added: [], removed: [], changed: [] },
      },
      current: catalogue(HOSTILE),
    },
    issues: [{ name: HOSTILE, line: 12, kind: "name", message: "invalid name" }],
    recent: [],
    journal_corrupt_lines: 0,
    gate_errors_24h: 0,
    credentials: [],
    notices: [],
    anomalies: [],
    ...overrides,
  };
}

beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

describe("relay helpers", () => {
  it("tells a pointer click from a key", () => {
    expect(isPointerClick(new MouseEvent("click", { detail: 1 }))).toBe(true);
    expect(isPointerClick(new MouseEvent("click", { detail: 0 }))).toBe(false);
  });

  it("shows relay controls only when the relay is in use", () => {
    expect(relayInUse(null)).toBe(false);
    expect(relayInUse(view({ pending: null }))).toBe(false);
    expect(relayInUse(view())).toBe(true);
    expect(relayInUse(view({ pending: null, approved_hash: "bb" }))).toBe(true);
    expect(relayInUse(view({ pending: null, stopped: true }))).toBe(true);
  });

  it("writes diff values as one line of text", () => {
    expect(shown(null)).toBe("(none)");
    expect(shown("a")).toBe("a");
    expect(shown(["--check", "{group}"])).toBe(`["--check","{group}"]`);
  });

  it("splits a changed map into the keys that changed", () => {
    const params = fieldLines({
      field: "params",
      before: { group: { pattern: "lab-[a-z]+" }, playbook: { values: ["a.yml"] } },
      after: { group: { pattern: "lab-[a-z]+" }, playbook: { values: ["a.yml", "b.yml"] }, host: { values: ["web01"] } },
    });
    expect(params).toEqual([
      { label: "params.host", before: "(none)", after: `{"values":["web01"]}` },
      { label: "params.playbook", before: `{"values":["a.yml"]}`, after: `{"values":["a.yml","b.yml"]}` },
    ]);
    expect(fieldLines({ field: "timeout_s", before: 540, after: 600 })).toEqual([
      { label: "timeout_s", before: "540", after: "600" },
    ]);
    expect(fieldLines({ field: "args", before: ["a"], after: ["b"] })).toHaveLength(1);
  });
});

describe("RelayStore", () => {
  it("approves the displayed hash and keeps a refusal to explain it", async () => {
    const store = new RelayStore();
    store.relay = view();
    vi.mocked(ipc.approveCatalogue).mockRejectedValueOnce({
      code: "catalogue_changed",
      message: "changed since it was displayed",
    });
    await store.approve("aa11");
    expect(ipc.approveCatalogue).toHaveBeenCalledWith("aa11");
    expect(store.refused).toEqual({ hash: "aa11", message: "changed since it was displayed" });
    expect(store.changedAgain).toBe(false);
    store.relay = view({ pending: { ...view().pending!, hash: "cc22" } });
    expect(store.changedAgain).toBe(true);
  });

  it("stops and resumes through the backend", async () => {
    const store = new RelayStore();
    await store.setStopped(true);
    expect(ipc.setRelayStopped).toHaveBeenCalledWith(true);
    vi.mocked(ipc.setRelayStopped).mockRejectedValueOnce("journal locked");
    await store.setStopped(false);
    expect(store.stopError).toBe("journal locked");
  });
});

describe("errorMessage", () => {
  it("reads an IpcError's message, and anything else as text", () => {
    expect(ipc.errorMessage({ code: "journal_unavailable", message: "journal locked" })).toBe("journal locked");
    expect(ipc.isIpcError({ code: "refused" })).toBe(false);
    expect(ipc.errorMessage("a fault of the IPC layer")).toBe("a fault of the IPC layer");
  });
});

describe("RelayBanners", () => {
  function banner(relay: RelayView) {
    const store = new RelayStore();
    store.relay = relay;
    return { store, ...render(RelayBanners, { store, nowMs: NOW }) };
  }

  it("approves on a pointer click only", async () => {
    const { getByTestId } = banner(view());
    const approve = getByTestId("approve-catalogue");
    await fireEvent.click(approve, { detail: 0 });
    expect(ipc.approveCatalogue).not.toHaveBeenCalled();
    await fireEvent.keyDown(approve, { key: "Enter" });
    await fireEvent.keyDown(approve, { key: " " });
    expect(ipc.approveCatalogue).not.toHaveBeenCalled();
    await fireEvent.click(approve, { detail: 1 });
    expect(ipc.approveCatalogue).toHaveBeenCalledWith("aa11");
  });

  it("renders catalogue text as text, never as markup", () => {
    const { container, getByTestId } = banner(view());
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    expect(getByTestId("action-added").textContent).toContain(HOSTILE);
    expect(getByTestId("catalogue-issues").textContent).toContain(HOSTILE);
  });

  it("shows a stopped relay without a pending change", () => {
    const { queryByTestId, getByTestId } = banner(view({ pending: null, stopped: true, stopped_since_ms: NOW }));
    expect(getByTestId("relay-stopped-banner").textContent).toContain("The relay is stopped since");
    expect(queryByTestId("catalogue-banner")).toBeNull();
  });
});
