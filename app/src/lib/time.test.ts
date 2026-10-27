import { describe, expect, it } from "vitest";
import { ago, duration, weekElapsedPercent } from "./time";

const MIN = 60_000;

describe("duration", () => {
  it("rounds down to the largest useful unit", () => {
    expect(duration(30_000)).toBe("now");
    expect(duration(4 * MIN)).toBe("4 min");
    expect(duration(120 * MIN)).toBe("2 h");
    expect(duration(130 * MIN)).toBe("2 h 10");
    expect(duration(125 * MIN)).toBe("2 h 05");
    expect(duration(3 * 24 * 60 * MIN)).toBe("3 d");
  });

  it("never shows a negative duration", () => {
    expect(duration(-5 * MIN)).toBe("now");
  });
});

describe("ago", () => {
  it("reads naturally", () => {
    expect(ago(1000, 1000)).toBe("just now");
    expect(ago(0, 31 * MIN)).toBe("31 min ago");
  });
});

describe("weekElapsedPercent", () => {
  const week = 7 * 24 * 60 * MIN;
  it("derives the elapsed share from the reset time", () => {
    const reset = week / 1000;
    expect(weekElapsedPercent(reset, 0)).toBe(0);
    expect(weekElapsedPercent(reset, week / 2)).toBe(50);
    expect(weekElapsedPercent(reset, week * 2)).toBe(100);
  });
});
