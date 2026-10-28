// The fixture index (D-181): `scripts/e2e-fixtures.mjs` writes
// `.fixtures/index.json`, each output file with its kind, from its own
// scenario tables. Vitest and Playwright both find fixtures through this
// loader, so no test holds a list of fixture names. No Playwright import:
// Vitest loads this module too.
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

/** `e2e/.fixtures`. Under Vitest's jsdom environment a module's URL is not
 *  a file URL, so the folder is then found from the app folder the runner
 *  starts in, as Playwright's is. */
export const FIXTURE_DIR = import.meta.url.startsWith("file:")
  ? join(dirname(fileURLToPath(import.meta.url)), ".fixtures")
  : join(process.cwd(), "e2e", ".fixtures");

export type FixtureKind = "board" | "relay" | "registry";

export interface FixtureIndex {
  files: Record<string, FixtureKind>;
}

const REBUILD = "run `pnpm e2e:fixtures` first";

/** The index in `dir`; throws, naming the command that writes it, when it
 *  is missing, unreadable or lists no board. */
export function readFixtureIndex(dir: string = FIXTURE_DIR): FixtureIndex {
  const path = join(dir, "index.json");
  if (!existsSync(path)) throw new Error(`no fixture index at ${path}: ${REBUILD}`);
  let index: FixtureIndex;
  try {
    index = JSON.parse(readFileSync(path, "utf8")) as FixtureIndex;
  } catch (error) {
    throw new Error(`the fixture index at ${path} cannot be read (${String(error)}): ${REBUILD}`, { cause: error });
  }
  if (!Object.values(index.files ?? {}).includes("board")) {
    throw new Error(`the fixture index at ${path} lists no board: ${REBUILD}`);
  }
  return index;
}

/** Every fixture of `kind`, by file name without `.json`, parsed. */
export function fixturesOf<T>(kind: FixtureKind, dir: string = FIXTURE_DIR): { name: string; data: T }[] {
  const index = readFixtureIndex(dir);
  return Object.entries(index.files)
    .filter(([, k]) => k === kind)
    .map(([file]) => ({
      name: file.replace(/\.json$/, ""),
      data: JSON.parse(readFileSync(join(dir, file), "utf8")) as T,
    }));
}
