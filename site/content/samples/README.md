# Code samples for the documentation pages

One file per sample. `site/scripts/generate-highlight.mjs` reads every file in
this directory, highlights it with the same tokeniser the component snippets go
through, and writes the result into `site/generated/highlight.json` under its
file name without the extension. The extension decides the language.

Written by hand, unlike the component snippets, which are cut from the gallery.
These show how to reach the library from outside it — installing it, opening a
window, observing cues — which is code no story contains. Keep them short and
keep them true: nothing here is compiled, so a stale sample is only caught by
someone reading it.
