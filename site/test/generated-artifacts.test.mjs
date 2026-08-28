import assert from "node:assert/strict";
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { compareGenerated } from "../../script/check-generated.mjs";

test("freshness catches every output byte and stale/missing files without repairing them", async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), "gpui-ai-freshness-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const expected = path.join(root, "expected"), actual = path.join(root, "actual");
  const download = "site/public/themes/example.json", pin = "site/generated/build.json";
  for (const [file, bytes] of [[download, '{"color":"#ffffff"}\n'], [pin, '{"commit":"original"}\n']]) {
    await mkdir(path.dirname(path.join(expected, file)), { recursive: true });
    await writeFile(path.join(expected, file), bytes);
  }
  await cp(expected, actual, { recursive: true });
  assert.deepEqual(await compareGenerated(expected, actual), []);
  for (const file of [download, pin]) {
    const previous = await readFile(path.join(actual, file));
    await writeFile(path.join(actual, file), "corrupt same-named bytes");
    for (let run = 0; run < 2; run++) {
      assert.deepEqual(await compareGenerated(expected, actual), [`changed: ${file}`]);
      assert.equal(await readFile(path.join(actual, file), "utf8"), "corrupt same-named bytes");
    }
    await writeFile(path.join(actual, file), previous);
  }
  await rm(path.join(actual, download));
  await writeFile(path.join(actual, "site/public/themes/stale.json"), "{}");
  for (let run = 0; run < 2; run++) assert.deepEqual(await compareGenerated(expected, actual), [
    `missing: ${download}`, "unexpected: site/public/themes/stale.json",
  ]);
});
