/** Validate native adapter fixtures against the actual checked-out ZCode schemas. */
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

type Result = { success: boolean; error?: { message: string } };
type Schema = { safeParse(value: unknown): Result };
type Case = { id: string; schema: string; valid: boolean; value: unknown };

async function main() {
  const args = process.argv.slice(2);
  const option = (name: string) => {
    const index = args.indexOf(name);
    if (index < 0) return undefined;
    if (!args[index + 1] || args[index + 1].startsWith("--")) {
      throw new Error(`Missing value for ${name}`);
    }
    return args[index + 1];
  };
  for (let index = 0; index < args.length; index += 2) {
    if (!["--drowzy", "--fixtures"].includes(args[index])) {
      throw new Error(`Unknown argument: ${args[index]}`);
    }
  }
  const drowzy = option("--drowzy");
  if (!drowzy) throw new Error("Usage: verify-zcode-fixtures.mts --drowzy <checkout> [--fixtures <json>]");
  const here = dirname(fileURLToPath(import.meta.url));
  const fixturePath = resolve(option("--fixtures") ?? resolve(here, "../fixtures/zcode-v4/cases.json"));
  const source = resolve(drowzy, "packages/shared/src/zcode-protocol-v4");
  const load = (name: string) => import(pathToFileURL(resolve(source, `${name}.ts`)).href);
  const [transport, command, snapshot, delta, rows, index] = await Promise.all(
    ["transport", "command", "snapshot", "delta", "rows", "sessions-index"].map(load),
  );
  const modules = [transport, command, snapshot, delta, rows, index];
  const cases: unknown = JSON.parse(await readFile(fixturePath, "utf8"));
  if (!Array.isArray(cases) || cases.length === 0) throw new Error("Fixture manifest must be a nonempty array");
  const ids = new Set<string>();
  let positive = 0;
  let negative = 0;
  let failed = 0;
  for (const candidate of cases) {
    const entry = candidate as Case;
    if (!entry || typeof entry.id !== "string" || !entry.id || ids.has(entry.id)
      || typeof entry.schema !== "string" || typeof entry.valid !== "boolean"
      || !Object.hasOwn(entry, "value")) throw new Error("Malformed or duplicate fixture entry");
    ids.add(entry.id);
    let result: Result;
    if (entry.schema === "parseCommandEnvelope") {
      const parsed = command.parseCommandEnvelope(entry.value);
      result = { success: parsed.ok, error: parsed.error };
    } else {
      const schema = modules.map((module) => module[entry.schema]).find(Boolean) as Schema | undefined;
      if (!schema || typeof schema.safeParse !== "function") throw new Error(`Unknown schema: ${entry.schema}`);
      result = schema.safeParse(entry.value);
    }
    if (entry.valid) positive++; else negative++;
    if (result.success !== entry.valid) {
      failed++;
      process.stderr.write(`FAIL ${entry.id}: expected ${entry.valid ? "valid" : "invalid"}; ${result.error?.message ?? "unexpectedly accepted"}\n`);
    } else process.stdout.write(`PASS ${entry.id}\n`);
  }
  if (positive === 0 || negative === 0) throw new Error("Positive and negative schema fixtures are both required");
  process.stdout.write(`${cases.length} cases: ${positive} positive, ${negative} negative, ${failed} failed\n`);
  if (failed) process.exitCode = 1;
}

main().catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
