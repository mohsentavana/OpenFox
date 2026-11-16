# Architecture

Twenty-four crates, each with one job, each module in one file. The map
is the module system: if you want to know where a decision is made, the
directory name is the answer.

## Crate map

```
                        openfox (bin)
              CLI - wiring - headless - about: pages
      ┌───────────────┬─────┴──────────┬───────────────┐
      ▼               ▼                ▼               ▼
 openfox-core    openfox-mcp     openfox-ui      openfox-platform
 tabs - session  MCP server      Rune chrome    fonts - lifecycle
 commands - zoom 32 tools        tabstrip..find  windows - android
 find - profile  jsonrpc         dialogs         deeplink - notify
 permissions     stdio + http
      │               │                │
      │               │                │
 openfox-net ◀── openfox-ext      openfox-render
 H1 + H2 + TLS   WASM runtime    wgpu compositor
 dns + doh - cookie  webext map  text - images
 cache - hsts - websocket        animation - video
 proxy - cert
      │
      ▼
 openfox-privacy  ── etp - classify - fingerprint - consent
 openfox-adblock  ── rules - engine - cosmetics - scriptlets
      │
      ▼
 openfox-engine   ── html - dom - css - style - layout - paint
 events - js bridge - selection - iframe - a11y
      │
      ▼
 openfox-render   ── wgpu compositor - glyph atlas - PNG
      │
      ▼
 openfox-devtools ── FDP - console - waterfall - profiler
 inspector - debugger - storage - a11y inspector
 openfox-storage  ── SQLite WAL - history - bookmarks
 forms - metadata - migrations
 openfox-passwords ── vault (AES-GCM + Argon2id) - breach (k-anon)
 openfox-sync     ── E2E envelopes - three-way merge - backoff
 openfox-reader   ── readability - typography - per-site memory
 openfox-media    ── PiP - media session - playlist - codecs
 openfox-pdf      ── xref - content streams - outline - viewer
 openfox-download ── manager - queue (token bucket) - range resume
 openfox-screenshot ── capture (in-house PNG) - annotate - share policy
 openfox-print    ── paged media - preview - in-house PDF output
 openfox-rtc      ── peer - sdp - ice (tokio UDP) - data channels
 openfox-updater  ── signed manifests - deltas - atomic swap
 openfox-crash    ── minidumps - torn-tail recovery - opt-in reporter
```

## One navigation, end to end

1. `openfox-core` receives `Command::Navigate { tab, url }` from the UI loop,
   the MCP server, or an extension. The tab enters `LoadState::Loading`
   and `NavigationStarted` goes out on the event bus.
2. The shield pipeline runs in order, before any socket: `openfox-privacy`
   classifies the request and applies the site's ETP level, `openfox-adblock`
   checks network rules (scriptlets and cosmetic rules stage for the
   document), `openfox-net::hsts` answers upgrade first, then
   `openfox-privacy::httpsupgrade` policy above it. A blocked request
   increments the tab's shield counters and never reaches the network.
   The ad dies at the policy engine, not in a renderer where it was
   already paid for.
3. `openfox-net::Client::fetch` resolves the host through the DNS cache
   (or `doh.rs` when DNS-over-HTTPS is on), consults `cache.rs` for a
   fresh RFC 9111 entry, picks a pooled connection, negotiates HTTP/2
   over rustls when the server offers it, and falls back to HTTP/1.1.
   Privacy headers, first-party cookies, and proxy routing
   (`proxy.rs`, CONNECT tunneling for https) attach here. `cert.rs`
   summarizes the peer certificate for the viewer and the error
   taxonomy.
4. `openfox-engine` tokenizes the HTML into an arena DOM (frames under
   `iframe.rs` get their own arena when cross-origin), parses and
   cascades style, builds a taffy box tree, solves layout, and paints a
   display list. `a11y.rs` builds the accessibility tree from DOM plus
   computed style — the same tree screen readers and `fox.a11y.tree`
   serve.
5. `openfox-render` composites the display list with wgpu — text through
   the shaping pipeline (`text.rs`), images through the bounded decode
   pool (`images.rs`), video frames through the YUV import path
   (`video.rs`), animations on the compositor tick (`animation.rs`).
   Desktop renders to a texture inside the Rune chrome; headless renders
   the same list to PNG. `openfox-reader` can swap the viewport for the
   extracted article; `openfox-pdf` feeds its own display items into the
   same compositor for PDF pages.
6. `openfox-storage` records the visit with frecency input, downloads
   (if any) run through `openfox-download`'s queue with range resume,
   `openfox-devtools` appends the network record and console output,
   and `openfox-core` broadcasts `NavigationCompleted` to whichever of
   the three front-ends is attached: the Rune chrome, the MCP server,
   or the FDP devtools socket.

## Threading

- One Tokio runtime owns the network: DNS, DoH, TLS, H2 multiplexing,
  the RTC sockets. Connections are pooled per `(scheme, host, port)` and
  timed out at the request, not the connection.
- The browser core is single-threaded by design, driven by the
  `CommandBus` in, `EventBus` (tokio broadcast) out. The Rune chrome,
  the MCP server, devtools and the WASM extension runtime are all just
  producers and consumers of those two buses. A tab crash is a
  `LoadState::Failed`, never a process exit.
- The compositor runs on the frame thread with its own wgpu device. The
  page texture and the chrome never fight over a context.

## Memory policy

- DOM lives in an arena (`Vec<Node>` with `NodeId` indices). No
  `Rc<RefCell<>>` webs, no reference cycles, no leak-hunting.
- Glyph atlas is a fixed-budget 1024x1024 R8 texture with LRU eviction;
  the image decode pool is byte-accounted with downscale-on-decode.
- The network log, console log, session history and download queue are
  ring buffers with explicit caps, not unbounded queues. The HTTP cache
  is LRU with a 64 MiB default budget.

## Error policy

- Library crates return typed errors via `thiserror`. `anyhow` is banned
  outside the `openfox` binary.
- A failed navigation is data, not an exception: it becomes a loadable
  error page in the tab, same pipeline, same display list.
- Sync refuses remote clocks more than five minutes ahead (a lying
  server must not win LWW fights). The updater refuses unsigned
  artifacts; crash recovery quarantines torn tails instead of trusting
  them.

## Dependency policy

Small, audited, pure-Rust where possible. Current rent-payers: `tokio`,
`rustls`, `h2`, `httparse`, `hickory-resolver`, `psl`, `taffy`, `wgpu`,
`ab_glyph`, `rusqlite`, `wasmtime`, `rune`, `blake3`, `argon2`,
`aes-gcm`, `x25519-dalek`, `ed25519-dalek`, `semver`, `sha1`/`sha2`.
Each is load-bearing for a subsystem in the map above. Nothing is
pulled in for convenience.

## Protocol surface

- **MCP** (`openfox-mcp`): JSON-RPC 2.0 over stdio and HTTP. 32 tools —
  navigation, DOM, screenshots, network, console, reader, downloads,
  media, permissions, certificates, cache, performance traces, the a11y
  tree, crash log. Agents get the same powers a user has, through the
  same command bus.
- **FDP** (`openfox-devtools`): the Fox Devtools Protocol — a CDP-shaped
  JSON protocol scoped to what OpenFox actually implements: Network,
  Console, Performance, DOM, Debugger, Storage, Accessibility. Devtools
  are a client of the browser, not a fork of it.

Both protocols are read-only by default and gated behind the
`--mcp-stdio`/`--mcp-http` flags. No remote surface is enabled unless
asked for.

## Honest deltas

Phase boundaries, stated plainly: page JavaScript (Boa behind the
`js.rs` ScriptHost trait), HTTP/3 (quinn), and full UAX#14 line
breaking are the named phase-2 items. The seams they will land in
already exist and are typed.
