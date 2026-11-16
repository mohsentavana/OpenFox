# REA — Reverse Engineering Anything

The method: take the two browsers that matter — Firefox for engine
discipline, Brave for shield discipline — tear them down to the
load-bearing walls, write down what the walls actually do, and rebuild
only that. Cleanly, in Rust. This file is the teardown ledger.

Tooling: `npx rea-agents setup` wires the REA MCP client (Ghidra, IDA,
jadx providers where installed); the Firefox census below was taken from
the `mozilla/gecko-dev` source tree and the Brave features map from
public Shields documentation. The numbers are the real ones.

## The census

Firefox (`mozilla/gecko-dev`) is 55,686+ indexed files at the top levels
alone. `dom/` carries 31,201 files across 60+ subsystems: canvas (9,645),
media (4,816), base (2,308), webgpu (1,436), html (1,146), security (946),
webidl (778), events (535), svg (508), serviceworkers (490), workers
(472), quota (451), indexedDB (413)... `devtools/` is 9,389 files
(client 7,090, server 731, shared 747). `browser/` is 8,775. Nobody
reads it all; everybody respects it. OpenFox's answer to that scale is
not fewer features — it is one language, one module discipline, and no
legacy: 24 crates, every module in one file, every file load-bearing.

Brave (Chromium + Shields) layers a privacy engine into the network
stack and a small economy on top of it. The engine is worth taking; the
economy is not.

## Firefox (Gecko)

### What Firefox does

- **Stylo.** Servo's parallel CSS engine, folded into Gecko: style is
  computed across cores with per-element sharing caches. The insight worth
  keeping: cascade is embarrassingly parallel if computed style is immutable
  per element.
- **WebRender.** Pages are compiled into a display list, the list is handed
  to a render thread, and the GPU eats rectangles. The DOM is not painted;
  a cached scene description is. Latency moves from paint to scene build,
  which is cacheable.
- **Quantum Flow.** Content process and chrome process are separate. The UI
  never janks because a page is parsing — the process boundary is the
  priority boundary.
- **Session restore.** Sessions are written crash-only: state is designed to
  survive the renderer dying, so "restore session" is the normal path, not
  the error path.
- **SpiderMonkey.** Interpreter, then baseline JIT, then Ion. Tiers, not one
  compiler. Startup never waits for the optimizing tier.
- **The long tail.** Passwords (Lockwise), sync (end-to-end encrypted),
  containers, reader mode, PDF viewer (pdf.js), picture-in-picture,
  downloads with resume, WebRTC, DevTools, an accessibility tree, about:
  pages. The tail is the product; the census above is its shape.

### What OpenFox keeps

- Display-list rendering (`openfox-render`): paint the list, not the DOM.
- Immutable computed style per element (`openfox-engine::style`), cascade
  ordered by specificity — the Stylo-shaped core, single-threaded for now,
  structured to parallelize later.
- Crash-only sessions (`openfox-core::session` + `openfox-crash::recover`):
  a session file is valid even if the last write died mid-line; the torn
  tail is quarantined, the rest is promoted.
- The tail, rebuilt where it is honest to rebuild it: passwords
  (`openfox-passwords`, AES-256-GCM under Argon2id, k-anonymous breach
  checks), sync (`openfox-sync`, X25519 + HKDF + AES-GCM envelopes, an
  untrusted server), reader mode (`openfox-reader`), PDF
  (`openfox-pdf`, in-house object model, no pdf.js), PiP and media
  session (`openfox-media`), downloads with range resume
  (`openfox-download`), WebRTC's protocol layer (`openfox-rtc`), the
  full devtools set (`openfox-devtools`), containers
  (`openfox-core::profile`), an accessibility tree that serves users and
  agents (`openfox-engine::a11y`), and about: pages that render through
  the engine like any page (`openfox::about`).
- No tiers of JS yet — there is no page JS yet. Phase 2 evaluates Boa
  behind the `js.rs` ScriptHost trait. The chrome never runs page script,
  so tiering is a page problem, contained.

### What OpenFox drops

XUL and its history, NPAPI ghosts, pocket integrations, sponsored tiles,
telemetry opt-out wording games. A browser that ships what it ships.

## Brave (Chromium + Shields)

### What Brave does

- Shields run in C++ inside the network stack. Blocking at the moment the
  request would be created, before connection setup — not in a renderer
  where the ad was already downloaded and merely hidden.
- Cosmetic filtering: rule lists carry `##selector` entries; hidden elements
  collapse rather than leave holes.
- Debouncing and redirect tracking: known bounce chains are collapsed before
  the redirect fires.
- Farbling: fingerprintable surfaces (canvas, audio, font metrics) are
  randomized per-site deterministically, so reads are stable within a site
  and useless across sites.

### What OpenFox keeps

- Network-layer blocking (`openfox-adblock` called from `openfox-net`):
  the socket is never opened. The ledger counts requests blocked and bytes
  never transferred — the shield counter ticks on money actually saved.
- Cosmetic rule parsing and per-host selector maps (`openfox-adblock::cosmetic`),
  scriptlet injection with a trusted-name registry (`scriptlets.rs`), and
  redirect resources as data URIs (`resources.rs`).
- The full shields map in Rust (`openfox-privacy`): tracker classification
  by category, ETP levels with per-site exceptions, fingerprint farbling
  with deterministic per-(origin, session) seeds (canvas jitter, audio
  noise floor, font quantization), CMP auto-reject, bounce-redirect
  collapse, link-decoration stripping, HTTPS-only with failure memory.
- Exception rules (`@@`) and allowlists: a block engine is only trustworthy
  if its unblock path is equally rigorous.

### What OpenFox drops

Chromium's process-per-tab sprawl, the Blink/V8 maintenance treadmill, and
the attention economy: Brave substitutes its own ads into slots it frees.
OpenFox frees the slot. Nothing takes its place. Rewards, the wallet, the
VPN upsell — absent on purpose; `about:rights` says so in one paragraph.

## Honest deltas

Phase 1 has no page JavaScript and no HTTP/3. Those are phase 2 items with
chosen paths (Boa behind the `js.rs` trait, quinn) and no pretend dates.
Tor windows and IPFS are phase 3 candidates with no schedule. Everything
the docs claim as working is a crate in this workspace, and its code is
the claim.
