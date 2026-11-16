# OpenFox

A browser written in Rust, end to end. Engine, network stack, privacy
shields, adblock, wallet, local AI assistant, private relay, devtools,
sync, reader, PDF, media, print, realtime, updater, crash handling, and
the user interface. No Electron, no webview, no JavaScript in the chrome,
no ads, no telemetry.

OpenFox is built on [Rune](https://github.com/mohsentavana/rune) — the
native GPU-rendered Rust UI framework — for its chrome, and targets
Windows and Android first.

## What it is

- **Pure Rust.** Twenty-seven crates, each with one job, every module in
  one file. No C++ in the UI, no node runtime in the shell. Memory
  safety from the type system, not from a patch queue.
- **No ads, by construction.** Blocking runs in `openfox-net` before a
  socket is ever opened — the request dies at the filter engine, not in
  a renderer. Nothing is substituted into the freed slot.
- **Shields, complete.** Tracker classification, ETP levels, fingerprint
  farbling, CMP auto-reject, bounce tracking, HTTPS-only — the Brave
  teardown, rebuilt (`openfox-privacy`).
- **MCP-native.** The browser speaks the Model Context Protocol over
  stdio and HTTP: 32 tools covering navigation, DOM, screenshots,
  network, reader, downloads, media, permissions, certificates, cache,
  performance traces, the accessibility tree, and the crash log. An
  agent drives the same backend the window drives.
- **Programmer-first.** FDP (the Fox Devtools Protocol), a WASM extension
  runtime with a WebExtension compatibility map, inspector, debugger,
  network waterfall, frame profiler, storage and a11y inspectors.

## Status

Phase 1, expanded. The architecture is complete and the subsystems are
real — 27 crates, ~180 Rust files, ~25,000 lines:

| Subsystem | State |
| --- | --- |
| Network (HTTP/1.1, HTTP/2, TLS, DNS cache, DoH, cookies, RFC 9111 cache, HSTS, proxy, cert viewer, WebSocket) | working |
| Privacy shields (classification, ETP, farbling, consent, bounce, HTTPS-only) | working |
| Adblock engine (network + cosmetic rules, scriptlets, redirect resources) | working |
| HTML tokenizer, DOM, CSS cascade, layout, paint | working |
| DOM events, selection, iframes/sandbox, accessibility tree | working |
| Page compositor (wgpu), text pipeline, image pool, animations, video frames | working |
| MCP server (32 tools, stdio + HTTP transports) | working |
| Devtools (FDP, console, waterfall, profiler, inspector, debugger, storage, a11y) | working |
| Storage (SQLite WAL: history, bookmarks, favicons, forms, migrations) | working |
| Passwords (AES-256-GCM vault, Argon2id, k-anonymity breach checks) | working |
| Sync (E2E encrypted, three-way merge, backoff) | working |
| Reader mode (readability extraction, typography, per-site memory) | working |
| Media (PiP, media session, playlist, codec tables) | working |
| PDF viewer (in-house object model, content streams, outline, find) | working |
| Downloads (queue, token-bucket limits, HTTP range resume) | working |
| Screenshots (display-list capture, in-house PNG encoder, annotation) | working |
| Print (paged media, preview, in-house PDF output) | working |
| WebRTC protocol layer (SDP, ICE over tokio UDP, data channels) | working |
| Updater (signed manifests, binary deltas, atomic swap, rollback) | working |
| Crash handling (minidump writer, torn-tail recovery, opt-in reporter) | working |
| Web3 wallet (BIP-39 keyring, EVM + Solana accounts, dApp grants, personal_sign) | working |
| AI assistant surface (session model, local summarizer, Ollama / OpenAI-compatible providers, prompt-invasion hygiene) | working |
| Private relay (multi-hop circuits, hand-rolled SOCKS5 + HTTP CONNECT, per-tab isolation) | working |
| HTTPS-Only + De-AMP (port-aware rewrites, canonical lift) | working |
| TLS SPKI pinning (deliberately minimal pin store) | working |
| Extension runtime (WASM via wasmtime, WebExtension compat map) | working |
| Chrome UI (Rune: tab strip, toolbar, shield, library, findbar, menus, dialogs, settings, reader) | building on Rune `dev` |
| Page JavaScript (Boa behind the `js.rs` trait) | phase 2 |
| HTTP/3 (QUIC) | phase 2 |

## Build

Windows (MSVC, default rustup toolchain):

```
cargo build --release -p openfox
```

Android:

```
cargo ndk -t arm64-v8a -p android -o app/src/main/jniLibs build --release
```

Headless agent mode:

```
cargo run -p openfox -- --headless --mcp-stdio
```

Feature parity with brave-core is tracked honestly, item by item, in
`docs/BRAVE_PARITY.md`; the design system behind the chrome is
`docs/UI_DESIGN.md`.

See `docs/BUILD.md` for toolchain details and `docs/ARCHITECTURE.md` for
the crate map. `docs/REA.md` records what was reverse-engineered from
Firefox and Brave (including the gecko-dev source census), and what
OpenFox does with it.

## Workspace

| Crate | Job |
| --- | --- |
| `openfox` | App shell: CLI, wiring, headless loop, about: pages |
| `openfox-core` | Tabs, sessions, settings, command/event buses, zoom, find, profiles, containers, permissions |
| `openfox-net` | HTTP/1.1 + HTTP/2, rustls, DNS cache + DoH, cookies, HTTP cache, HSTS, proxy, cert viewer, WebSocket |
| `openfox-privacy` | The shields: classification, ETP, farbling, consent, bounce, HTTPS upgrades |
| `openfox-adblock` | Rule parser, network engine, cosmetic filters, scriptlets, redirect resources |
| `openfox-engine` | HTML, DOM, CSS cascade, layout, paint, events, JS bridge, selection, iframes, a11y |
| `openfox-ui` | Rune chrome: tab strip, toolbar, shield, downloads, library, findbar, menus, dialogs, settings |
| `openfox-platform` | Windows and Android integration, fonts, lifecycle, deep links, notifications |
| `openfox-render` | wgpu compositor, glyph atlas, text pipeline, image pool, animations, video |
| `openfox-mcp` | Model Context Protocol server: JSON-RPC, 32 tools, stdio + HTTP |
| `openfox-devtools` | FDP protocol, console, waterfall, profiler, inspector, debugger, storage, a11y |
| `openfox-storage` | SQLite WAL: history, bookmarks, favicons, forms, metadata, migrations |
| `openfox-passwords` | AES-256-GCM vault under Argon2id, form detection, autofill, k-anon breach checks |
| `openfox-sync` | End-to-end encrypted sync: envelopes, three-way merge, backoff |
| `openfox-ext` | WASM extension runtime, WebExtension compatibility map |
| `openfox-reader` | Reader mode: readability extraction, typography, per-site memory |
| `openfox-media` | PiP, media session, playlist, codec truth tables |
| `openfox-pdf` | PDF viewer: in-house xref/object model, content streams, outline |
| `openfox-download` | Download manager, bounded queue, HTTP range resume |
| `openfox-screenshot` | Display-list capture, in-house PNG encoder, annotation, share policy |
| `openfox-print` | Paged media, print preview, in-house PDF output |
| `openfox-rtc` | Peer sessions, SDP, ICE over tokio UDP, data channels |
| `openfox-updater` | Signed manifests, binary deltas, atomic swap with rollback |
| `openfox-crash` | Minidump writer, torn-tail recovery, opt-in reporter |
| `openfox-wallet` | Web3 wallet: BIP-39 keyring, secp256k1/ed25519 derivation, EIP-55 + base58 addresses, origin grants, signing |
| `openfox-ai` | Leo-style assistant: sessions, provider trait, local extractive summarizer, page context binding |
| `openfox-relay` | Tor-style private windows: circuits over SOCKS5/HTTP CONNECT, isolation keys, circuit pool |

## License

Source-available, not open-source. **All rights reserved to the project
owner.** You may run OpenFox for personal use, read every line, and
propose changes through pull requests that the project accepts or rejects
— contributions become the project's property, and forking or
redistribution is prohibited. The full terms are in
[LICENSE.md](LICENSE.md); the contribution process is in
[CONTRIBUTING.md](CONTRIBUTING.md).

## Unslop

This repo follows the [unslop](https://github.com/mohsentavana/unslop)
discipline, applied to code:

- No filler modules, no dead code, no comment noise.
- Every dependency pays rent in a subsystem that uses it.
- One accent, flat surfaces, honest numbers. The shield counter is the
  loudest element in the UI, and it counts something real.

## Credits

- [Rune](https://github.com/mohsentavana/rune) — native UI framework, MIT.
- [unslop](https://github.com/mohsentavana/unslop) — design discipline,
  CC BY 4.0, Damir Sokolovsky.
- Firefox and Brave — reverse-engineering references, documented in
  `docs/REA.md`.

MPL-2.0.
