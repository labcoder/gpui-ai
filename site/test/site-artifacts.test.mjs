// One integration entrypoint makes the shared artifact lifetime explicit:
// importing a helper from separate *.test.mjs processes would still build twice.
import "./build.checks.mjs";
import "./pages.checks.mjs";
import "./shell.checks.mjs";
