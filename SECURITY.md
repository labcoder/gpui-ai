# Security policy

## Supported versions

gpui-ai is pre-1.0 and ships from git rather than crates.io. Only the latest
`main` and the most recent tagged release receive fixes; there are no
maintenance branches for older tags.

## Reporting a vulnerability

Report privately through GitHub's
[security advisory form](https://github.com/labcoder/gpui-ai/security/advisories/new).
Please do not open a public issue for a vulnerability, and do not report it in a
pull request.

Include what you have: the affected component or module, the revision you tested
(`git rev-parse HEAD`), the platform, and the smallest reproduction you can
manage. A failing test or a short `gallery` story is ideal.

Expect an acknowledgement within a week. If a report is accepted, the fix lands
on `main` with a note in [CHANGELOG.md](CHANGELOG.md), and you are credited
unless you ask otherwise.

## Scope

gpui-ai is a user-interface library. It renders data the host application gives
it and reports typed events back; it opens no sockets, spawns no processes,
reads no files, and executes nothing it is handed. The interesting reports are
therefore about what the library does with hostile *content*:

- Text, markdown, code, or diff content that escapes its bounds, corrupts the
  surrounding layout, or is rendered as something other than text.
- Content that makes a component hang, exhaust memory, or panic — including
  markdown, unified diffs parsed by `DiffFile::from_unified`, and any snapshot a
  component accepts.
- A component reporting an event that does not match the interaction the user
  performed, since applications act on those events.
- Anything the library logs, copies to the clipboard, or renders that the host
  supplied as sensitive.

Out of scope here, though still worth reporting upstream: defects in
[gpui-component](https://github.com/longbridge/gpui-component) or
[GPUI](https://github.com/zed-industries/zed) reproducible without gpui-ai, and
anything in the `gallery` crate's simulated demo data, which is fixtures rather
than shipped behavior.
