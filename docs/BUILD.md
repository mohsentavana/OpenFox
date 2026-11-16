# Build

## Windows

MSVC toolchain (rustup default):

```
rustup default stable-x86_64-pc-windows-msvc
cargo build --release -p openfox
```

Install the Visual Studio C++ build tools rustup asks for, nothing more.

GNU toolchain (`x86_64-pc-windows-gnu`): `windows-link` (under `parking_lot`,
`libloading`, `windows-core`, `windows-sys`) imports with `raw-dylib`, and
rustc makes those import libraries with `dlltool` on GNU targets. The
`dlltool` shipped in rustup's `rust-mingw` cannot run without an `as` it does
not ship (rust-lang/rust#140704). Without binutils the build stops at
`windows-result` with `error calling dlltool 'dlltool.exe': program not found`.

```
pacman -S mingw-w64-ucrt-x86_64-binutils
```

Put `C:\msys64\ucrt64\bin` on `PATH`, or build with:

```
RUSTFLAGS=-Cdlltool=C:\msys64\ucrt64\bin\dlltool.exe cargo build --release -p openfox
```

Fonts: the platform layer loads `C:\Windows\Fonts\segoeui.ttf` (fallback:
`tahoma.ttf`) for the glyph atlas. No font is bundled.

## Android

Prerequisites:

- Android NDK r26+
- `JAVA_HOME` pointing at a JDK 17
- Rust targets:

```
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android
cargo install cargo-ndk
```

Build the core into JNI libs:

```
cargo ndk -t arm64-v8a -p android -o app/src/main/jniLibs build --release
```

Packaging is ordinary Gradle: the `openfox-platform` android module enters
through `android_main` with an `AndroidApp`, loads `fonts/OpenFox.ttf` from
the APK asset manager, and hands the event loop to the Rune chrome. Back
button routes to `Command::Back` when the tab history is non-empty, otherwise
to the Android system.

## Headless (agents, CI, servers)

```
cargo run -p openfox -- --headless --mcp-stdio
```

Stdio MCP, no window, no GPU surface required — `fox.screenshot` renders
through wgpu's surfaceless path to an Rgba8 texture, then encodes PNG on the
CPU. HTTP transport:

```
cargo run -p openfox -- --headless --mcp-http 9412
```

Test the tools:

```
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | \
  cargo run -p openfox -- --headless --mcp-stdio
```

## Toolchain pins

Rune is pre-release and pinned to a rev of `dev`:

```toml
rune = { git = "https://github.com/mohsentavana/rune", rev = "<pin>" }
```

`dev` is never rewritten, so a pin is stable. Bump deliberately, never
float.

## What is intentionally absent

- No bundled font, no bundled root store (rustls uses `webpki-roots`).
- No build script that downloads anything at compile time.
- No feature that phones home to check for updates.
