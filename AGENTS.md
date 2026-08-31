# proxybench

Desktop app (Tauri 2 + Svelte 5) that splits HTTP proxy lists by `/24` and
measures Connect + TTFB per subnet.

`README.md` is what and why for people installing the app. This file is how
to work here.

## Product

One window, not resizable. One workflow:

1. Drop a `.txt` file or a folder of `.txt` files, or Open files
   (`host:port:user:pass` HTTP only).
2. Table of IPv4 `/24` subnets. Country at import. Tags persist by CIDR.
   Inventory persists locally; Open files appends. Last probe metrics persist
   until that `/24` gains lines. The last HTTPS target is remembered.
3. Select `/24`s with click, Cmd/Ctrl-click, or Shift-click across the current
   sorted and filtered rows; Cmd/Ctrl-A selects all filtered rows and Escape
   clears selection. Selections persist through sort, filter, and pages.
   Probe all, selected, or one `/24` → OK + Connect p50/p95 + TTFB
   p50/p95 + last probe. 5 s timeout, 5 starts/s, 32 in flight.
4. Export `[{tags}_]{CC}_{IP}_24_{qty}.txt`. Untagged files start with the
   country code. No country → `XX`.
5. Filter sits with Open files, Export, and Probe all. Toolbar Probe and
   Export act on selected rows when present. Probe progress and the version
   sit in the bottom bar. 15 subnets per page. Right-click a selected row to
   Probe or Export the selection; otherwise those actions apply to that row.
   Remove always applies to that row.

Probe is the only action verb for a run. Anything else is out of scope until
asked.

## Code

- Zero comments. Names and structure carry the meaning. The only allowed
  comment is `// MARK:` for section markers, plus one line when an external
  quirk cannot be encoded in code.
- One concern per file. Size follows cohesion. Split on mixed concerns, never
  on line count.
- Precise domain types: proxy line, subnet, tag. Not bare strings for those.
- Rust owns parse, split, country, probe, export, inventory, tags, and last
  target. App data files: `proxies.json`, `tags.json`, `last-target.txt`.
- Svelte is a thin view over Tauri commands. No parse, split, or probe logic
  in the frontend. Svelte 5 runes (`$state`, `$derived`, `$props`, `$effect`).
  One page, no router.
- Credentials never appear in the UI, logs, errors, or exported HTML.
- English for identifiers, docs, commits, and UI copy.
- Colour is emphasis, not a score: Connect/TTFB green under 500 ms, red over
  1500 ms; OK green at 80% success, red below 30%.

## Stack

| Layer   | Choice                                                                 | Not                                   |
| ------- | ---------------------------------------------------------------------- | ------------------------------------- |
| Package | bun 1.3.14                                                             | npm, yarn, pnpm                       |
| Rust    | 1.88.0 via `rust-toolchain.toml`                                       | floating stable                       |
| Shell   | Tauri 2                                                                | egui, iced, Electron                  |
| UI      | Svelte 5 + Tailwind 4 + TanStack Table                                 | router, extra pages, component kits   |
| Probe   | tokio + HTTP CONNECT + rustls (ring) + hyper HTTP/1                    | reqwest, libcurl, isahc               |
| Country | `GET https://api.country.is/{ip}` on one IP per `/24`, direct (`ureq`) | through-proxy lookup, GeoLite in-repo |
| Tags    | local JSON keyed by CIDR                                               | filename-as-identity                  |

Connect = TCP to the proxy + CONNECT `200`, before origin TLS.
TTFB = same start Instant until origin HTTP/1 headers; body is never read.
Failed probes are dropped from percentiles. OK is the success count. No
connection reuse. Each probe times out after 5 seconds.

## Do not

- SOCKS, URL proxy lines, extra formats, hostname hosts
- Multi-target, `targets.csv`, ASN, DNS/TLS columns, status/error columns
- Quality scores, bypass claims, a web server
- `reqwest` on the speed path
- Ship a MaxMind database
- Linux installers, extra pages, window resize
- `git add -A` / `git add .`

## Docs

- `README.md` is the product page: install and use, no toolchain. Hero
  assets live in `docs/assets/` (app icon + window screenshot).
- This file is how to work here. Do not duplicate README copy.
- Never restate code logic in docs. Update docs in the same change as the
  behaviour they describe.
- No horizontal rules, no emoji in docs.

## Tests

- Domain only: parse, split, probe timings, inventory, export. No UI tests
  unless asked.
- Tests live beside the code they cover. Name the case.
- CI lints and tests every pull request and `main`.

```text
bun run check
bun run build
cargo fmt --all --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --locked --lib --manifest-path src-tauri/Cargo.toml
```

## Commits

- [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):
  `type(scope): summary`.
- Stage explicit paths. Review `git diff --cached` before committing.
- Atomic commits, one purpose each.
- Call @oracle for a review before EVERY commit.

## Agents

- @designer for visual and interaction decisions. Window chrome, table
  density, and copy stay with the current UI.
- @oracle for high-risk technical advice and for that pre-commit review.
- @explorer / @librarian when discovery or current docs beat guessing.
- bun for JS; cargo via `src-tauri/Cargo.toml`. `bun run tauri dev` to run
  the app.

## Ship

Bump `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`
to the same `X.Y.Z`, merge to `main`, then tag `vX.Y.Z`.

GitHub Actions (`.github/workflows/ci.yml` on `main` and pull requests,
`.github/workflows/release.yml` on `vX.Y.Z`) builds Developer ID signed and
Apple-notarized macOS DMGs for Apple Silicon and Intel and an unsigned Windows
NSIS setup wizard.

Updater releases require `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The workflow keeps releases as drafts
until `latest.json` contains macOS arm64, macOS x64, and Windows x64. Never
replace published assets; ship a higher patch version.

macOS releases require `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_SIGNING_IDENTITY`, `APPLE_API_KEY`, `APPLE_API_ISSUER`, and
`APPLE_API_PRIVATE_KEY`. The certificate and API private key are base64
encoded. Apple code signing is independent from updater signing.
