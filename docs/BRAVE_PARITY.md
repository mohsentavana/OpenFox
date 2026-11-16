# OpenFox ↔ brave-core feature parity map

brave-core (github.com/brave/brave-core) is the C++ Chromium-layer that
implements Brave's feature surface. This document maps every headline
brave-core subsystem to the OpenFox crate that owns it, and is honest about
what is not built yet. OpenFox is not a Chromium fork; it is a native Rust
browser, so "parity" means the capability, not the code.

| Brave feature | brave-core home | OpenFox home | State |
|---|---|---|---|
| Shields (adblock + tracker blocking) | `components/brave_shields` | `openfox-adblock` (network rules, cosmetic filters, scriptlets, resources) | built |
| Per-site shield controls | `brave_shields2` | `openfox-core` permissions + `openfox-ui/src/shield.rs` | built |
| Brave Rewards / BAT | `components/brave_rewards` | **not built — refused by charter** | see below |
| Brave Wallet (Web3, ETH/SOL) | `components/brave_wallet` | `openfox-wallet` (BIP-39 keyring, secp256k1 + ed25519 derivation, EIP-55 & base58 addresses, dApp origin grants, personal_sign, JSON-RPC balance requests behind a trait) | built (core surface) |
| Leo AI assistant | `components/brave_leo` | `openfox-ai` (session model, provider trait: local extractive summarizer / Ollama / OpenAI-compatible, page-context binding with prompt-invasion hygiene) | built (core surface) |
| Tor private windows | `brave/components/tor` | `openfox-relay` (multi-hop circuits over hand-rolled SOCKS5 + HTTP CONNECT, per-tab isolation keys, circuit pool with rotation) | built (core surface) |
| HTTPS upgrades / HTTPS-only mode | HTTPS-by-default net stack | `openfox-net/src/upgrade.rs` (`HttpsMode` Off/Upgrade/Only, port-aware rewrites) | built |
| De-AMP | `components/brave_de_amp` | `openfox-net/src/upgrade.rs` (`deamp_url`, canonical extraction) | built |
| Speedreader | `components/brave_speedreader` | `openfox-reader` (readability extraction, typography) | built |
| Playlist | `components/brave_playlist` | `openfox-media` (playlist model, media pipeline) | built |
| WebRTC / Brave Talk plumbing | `brave/browser/*talk*` | `openfox-rtc` (SDP, ICE over tokio UDP, data channels) | built |
| Sync (chain-based) | `components/brave_sync` | `openfox-sync` (X25519 device identity, HKDF per-collection subkeys, AES-GCM envelopes, cursors) | built |
| Updater with signature gates | `brave/update` | `openfox-updater` (dual ed25519 signatures: build key + release key, staged atomic swap) | built |
| Crash reporting | minidump plumbing | `openfox-crash` (minidump writer, recovery) | built |
| WebExtension support | Chromium extensions layer | `openfox-ext` (WASM sandboxed extensions, capability manifest, MV2+MV3 verdicts, API compat table) | built |
| DevTools | Chromium devtools | `openfox-devtools` (console, network, debugger, inspector, profiler, FDP protocol) | built |
| Password manager | `brave/components/password_manager` | `openfox-passwords` (AES-GCM vault, master password KDF, verifier) | built |
| Fingerprint farbling | `components/brave_fingerprinting` | `openfox-privacy` (first/third-party partitioning, header trimming in `openfox-net/src/privacy.rs`) | built (subset) |
| Private windows (ephemeral) | incognito plumbing | `openfox-core` profiles (`ProfileManager::ephemeral`, write-refusing) | built |
| Containers (tab isolation) | — (Firefox feature) | `openfox-core/src/profile.rs` (8-color containers, per-container cookie jars) | built |
| MCP / agent tooling | — (no equivalent) | `openfox-mcp` (stdio + HTTP transports, JSON-RPC, browser-as-a-tool) | built — beyond parity |
| TLS SPKI pinning | — (removed from Chrome) | `openfox-net/src/tls_pinning.rs` | built (deliberately minimal) |

## Refused by charter

- **Brave Rewards / BAT / sponsored images / private ads.** OpenFox's charter
  is *no ads, no attention monetization, ever* — including "privacy-respecting"
  ads. `openfox-wallet` exists for custody and dApp use, not for an ad economy.
- **Sponsored tiles, affiliate links, referral codes in suggestions.** Same
  clause.

## Not built yet (honest list)

- IPFS/IPRS gateway integration and `.ipfs` scheme handling.
- Full EIP-1559 gas estimation UI (the RPC request builder exists in
  `openfox-wallet/src/balances.rs`).
- Circuit scheduler heuristics beyond per-session/per-N rotation in
  `openfox-relay`.
- The Brave Talk frontend itself (the RTC substrate exists in `openfox-rtc`).

Each missing item is a crate boundary away — that is the point of the
one-concern-per-file architecture.
