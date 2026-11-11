import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { after, before, test } from "node:test";

const here = dirname(fileURLToPath(import.meta.url));
const drowzy = process.env.DROWZY_ROOT;
assert.ok(drowzy, "DROWZY_ROOT must identify the authoritative schema checkout");
let scratch;
let cases;
before(async () => {
  scratch = await mkdtemp(join(tmpdir(), "goya-schema-regression-"));
  cases = JSON.parse(await readFile(resolve(here, "../fixtures/zcode-v4/cases.json"), "utf8"));
});
after(async () => {
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

async function run(entries, extra = []) {
  const args = ["--import", resolve(drowzy, "node_modules/tsx/dist/loader.mjs"),
    resolve(here, "verify-zcode-fixtures.mts"), "--drowzy", drowzy];
  if (entries !== undefined) {
    const path = join(scratch, `${crypto.randomUUID()}.json`);
    await writeFile(path, JSON.stringify(entries));
    args.push("--fixtures", path);
  }
  args.push(...extra);
  return new Promise((resolveResult, reject) => {
    const env = { ...process.env };
    // 子校验器不是测试 worker，不能继承父 runner 的内部序列化通道。
    delete env.NODE_TEST_CONTEXT;
    const child = spawn(process.execPath, args, { env, stdio: ["ignore", "pipe", "pipe"] });
    let output = "";
    child.stdout.setEncoding("utf8").on("data", (chunk) => { output += chunk; });
    child.stderr.setEncoding("utf8").on("data", (chunk) => { output += chunk; });
    child.once("error", reject);
    // exit 先于 stdout/stderr 关闭；close 才能证明诊断已完整收集。
    child.once("close", (code) => resolveResult({ code, output }));
  });
}

test("canonical positive and negative cases match current authoritative schemas", async () => {
  const result = await run();
  assert.equal(result.code, 0, result.output);
  assert.match(result.output, /0 failed/);
});

test("an invalid send payload labeled valid fails through payload parsing", async () => {
  const corrupted = structuredClone(cases);
  corrupted.find((entry) => entry.id === "command.send").value.payload.text = 42;
  const result = await run(corrupted);
  assert.notEqual(result.code, 0);
  assert.match(result.output, /FAIL command.send/);
});

test("empty corpus cannot certify compatibility", async () => {
  const result = await run([]);
  assert.notEqual(result.code, 0);
  assert.match(result.output, /nonempty array/);
});

test("unknown schema names fail rather than silently skipping coverage", async () => {
  const corrupted = structuredClone(cases);
  corrupted[0].schema = "missingSchema";
  const result = await run(corrupted);
  assert.notEqual(result.code, 0);
  assert.match(result.output, /Unknown schema/);
});

test("duplicate IDs cannot overwrite an earlier fixture result", async () => {
  const result = await run([...cases, cases[0]]);
  assert.notEqual(result.code, 0);
  assert.match(result.output, /duplicate fixture/);
});

test("a positive-only corpus fails its negative coverage gate", async () => {
  const result = await run(cases.filter((entry) => entry.valid));
  assert.notEqual(result.code, 0);
  assert.match(result.output, /Positive and negative/);
});

test("unknown arguments cannot accidentally bypass the requested fixture", async () => {
  const result = await run(undefined, ["--fixture", "ignored.json"]);
  assert.notEqual(result.code, 0);
  assert.match(result.output, /Unknown argument/);
});
