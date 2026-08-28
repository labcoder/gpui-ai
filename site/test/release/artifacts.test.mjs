import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { assertParity, inlinePin } from "../../../crates/gallery-web/www/test-support/scheme-parity.mjs";

test("the freshly built embed preserves the full inline scheme matrix", async () => {
  // ENOENT is a failure. This suite is discovered only after the release build,
  // never opportunistically against a developer's old dist.
  const built = new URL("../../../crates/gallery-web/www/dist/embed.html", import.meta.url);
  const html = await readFile(built, "utf8");
  assertParity(inlinePin(html, "built embed"), "built embed");
});
