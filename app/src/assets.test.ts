// The app's logos are the published ones (AD-67): every image of
// src/assets is a byte copy of the file of the same name in docs/assets.
/// <reference types="node" />
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const APP_ASSETS = join(process.cwd(), "src", "assets");
const DOCS_ASSETS = join(process.cwd(), "..", "docs", "assets");

describe("assets", () => {
  it("the app's logos are byte copies of docs/assets", () => {
    const names = readdirSync(APP_ASSETS);
    expect(names).toContain("logo.svg");
    expect(names).toContain("logo-dark.svg");
    for (const name of names) {
      expect(readFileSync(join(APP_ASSETS, name)).equals(readFileSync(join(DOCS_ASSETS, name))), name).toBe(true);
    }
  });
});
