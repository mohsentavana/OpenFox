# Contributing to OpenFox

Thank you for wanting to make OpenFox better. OpenFox is **source-available**:
you can read every line, and the only way to change the official tree is a
pull request that the project accepts. This document is the whole process —
there is no other path, and no forks.

## The deal, in one paragraph

You submit a pull request. The project reviews it and either **accepts** it
(merged code becomes part of OpenFox, owned by the project) or **rejects** it
(any reason, no appeals). By submitting, you assign the copyright of your
change to the project. You cannot distribute modified builds of OpenFox, and
you cannot fork it. The full legal terms are in [LICENSE.md](LICENSE.md) —
read Section 4 before your first PR; submitting means you agree to it.

## Ground rules

1. **Rust only.** The browser is pure Rust. A PR adding a non-Rust runtime
   dependency for something the ecosystem already solves in Rust will be
   rejected.
2. **One concern, one file.** The architecture keeps every important module
   in its own `.rs` file with a `//!` doc-comment explaining the design
   decision it embodies. New code follows the same shape.
3. **No ads, no tracking, ever.** Any change that monetizes attention — ad
   slots, sponsored tiles, telemetry with identity, "anonymous" profiling —
   is rejected on sight and the issue closed. This is the charter.
4. **`cargo check` passes.** The whole workspace must compile clean before
   you push. `cargo fmt` and `cargo clippy` clean is appreciated.
5. **Quality of comments counts.** A PR that works but cannot explain itself
   will be asked to explain itself. The doc comments are the architecture.

## What gets accepted fastest

- Bug fixes with a reproducing description.
- Ad-block list and scriptlet improvements.
- Performance work with numbers (before/after, what was measured).
- New WebExtension API coverage in `openfox-ext`.
- MCP tool coverage in `openfox-mcp`.
- Platform integration fixes (Windows, Android).
- Layout/paint correctness fixes in `openfox-engine` / `openfox-render`.

## What needs a design discussion first

Open an issue before writing code for: new crates, public API changes,
permission model changes, anything touching `openfox-net`'s trust decisions,
or anything with privacy implications. PRs that skip the discussion for these
areas usually get rejected for scope, not code quality.

## Review and verdict

A maintainer reviews your PR. Possible outcomes:

- **Accepted** — merged, possibly with edits. Your change ships under the
  OpenFox name and license.
- **Changes requested** — the door stays open; push to the same branch.
- **Rejected** — closed with a reason. That reason is final for this PR.
  You are free to open a different PR on the same theme.

The maintainers owe you a review, not a merge.

## Reporting security issues

Do not open public issues for vulnerabilities. The repository's security
policy file lists the private channel. You will be credited in the release
notes if you wish.

---

*OpenFox is ad-free by charter, private by architecture, and owned by the
project — contributions are welcome, forks are not.*
