# Security policy

## Reporting a vulnerability

Do not open a public issue for anything security-sensitive. Email the
maintainer at the address on the repository owner's profile with:

- what you found, with steps or a proof of concept;
- the affected version (Help → About, or `openfox --version-print`);
- whether it is already public.

You will get an acknowledgment within 7 days. Coordinated disclosure: we
agree a timeline, fixes land in a patch release, and you are credited in
the release notes if you wish.

## Scope

In scope: the engine (`openfox-engine`), the network stack and its trust
decisions (`openfox-net`, including TLS, HSTS, cookies, upgrades,
pinning), the extension sandbox (`openfox-ext`), the password vault
(`openfox-passwords`), the updater's signature chain
(`openfox-updater`), the wallet keyring (`openfox-wallet`), and the
relay's isolation model (`openfox-relay`).

Out of scope: the vendored Rune UI framework (report upstream — we pin a
rev and pick up its fixes), and anything requiring physical access to an
unlocked machine.

## The guarantees we hold

- No ads, no telemetry, no attention monetization — a PR that adds any
  of these is rejected on sight; a vulnerability "caused" by their
  absence is not a vulnerability we recognize.
- The updater refuses binaries without both signatures (build key +
  release key). There is no TOFU window.
- Extensions run in WASM with a capability manifest; a sandbox escape is
  critical severity, always.
