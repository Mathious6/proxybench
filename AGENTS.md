# Repository Guide

- This guide never cites code: no paths, symbols or snippets. The code is the reference; read it before acting.
- Keep everything simple: this guide, the code and the stack. When in doubt, choose the option with fewer parts.

## Guide Governance

- This guide is under human supervision. Never edit it unless the owner asks; propose changes in your report instead.
- Every line is a bullet of 30 words at most: no paragraphs, tables, code blocks or nested bullets.
- State principles that survive refactors. If a bullet breaks when code is renamed, it is wrong.
- Describe the current rule only: no history, dates, versions shipped or "previously".
- One fact, one place. The README is the product page for people installing the app; this guide is how to work here.
- Add a bullet only when an agent would otherwise get it wrong. Keep each section under about ten bullets, Resources excepted.

## Priorities

- When rules conflict, decide in this order: privacy, correctness of measurements, simplicity, reuse of what exists, performance.
- When code contradicts this guide, the guide wins: fix the code you touch in the same change.
- When the guide is silent, follow the linked resources, then the closest existing code, then ask.

## Build Mode

- No external client or API depends on this app: breaking changes are free. Optimize for the clearest design, never for continuity.
- No legacy support: no aliases, shims, migrations of internal formats, compatibility flags or dual implementations. Change every caller in the same change.
- The owner's local inventory and tags files must keep loading; new fields default, removed fields are ignored.
- Zero references to replaced code or past choices, in code, tests, names or docs. Git history is the archive.

## Product

- One fixed-size window, one page, dark only. A small desktop tool for people who manage HTTP proxy lists by IPv4 /24.
- Accepted line format is host:port:user:pass, HTTP proxies, IPv4 hosts. Everything else is skipped and counted.
- Two run verbs only: Check proves a /24 is alive through a few proxies; Probe measures Connect and TTFB through every proxy.
- Check runs on launch and after import when enabled in Settings. Settings own the target, limits, timeout and check sample.
- Connect ends when the proxy answers CONNECT with 200. TTFB ends when origin headers arrive. Bodies are never read; connections are never reused.
- Failed probes are dropped from percentiles and counted against OK. Growing a /24 clears its health and probe results.
- Export writes the source lines verbatim, named by tags, country and subnet; several subnets bundle into one archive. AYCD JSON is the only other format.
- Colour is emphasis, never a score: health uses dots, timings colour only outliers, OK colours on success ratio.

## Out Of Scope

- SOCKS, proxy URLs, hostnames, IPv6, extra line formats, multi-target runs, ASN, DNS or TLS columns, status or error columns.
- Quality scores, bypass claims, a web server, a bundled geolocation database, Linux installers, extra pages, window resize.

## Simplicity

- Write the fewest features, branches and edge cases that satisfy a real need. Deleting code is progress.
- SOLID in practice: one reason to change per module, narrow interfaces, dependencies passed in, never reached for globally.
- Prefer few complete, cohesive files over many fragments. Split only on mixed concerns, never on line count.
- Side effects live at the boundaries. Untrusted input, files and IPC payloads, is parsed once at the edge into domain types, then trusted.
- Make illegal states unrepresentable: closed enums and newtypes for proxy line, subnet, tag, target, health and settings; never bare strings or flag pairs.

## Naming

- One concept, one name, everywhere: Rust, TypeScript, UI copy and README use the same words.
- Modules are named after their concept; functions after the action they perform, as a verb.
- No vague names: no Manager, Helper, Util, Handler or Service suffixes, no data, info, item or obj.

## Never

- No catch-all utility modules, factories, event buses, speculative layers or abstraction built for one case.
- No boolean parameters: write two functions or model the choice as an enum.
- No comments, doc comments included. Only section markers, and one line when an external quirk cannot be encoded in code.
- No dead code, commented-out code, unused exports or features kept for later.

## Architecture

- Rust owns every rule: parsing, grouping, country lookup, check, probe, scheduling, inventory, tags, settings, export and persistence.
- The frontend is a thin view over Tauri commands and events. It holds view state only: sort, filter, selection, page, open dialogs.
- One frontend module talks to Tauri. A credential-free fixture stands in for it in a plain browser, for development and screenshots.
- App data lives in the platform app data directory as small JSON files written atomically with owner-only permissions.
- The probe path is tokio, hyper HTTP/1 and rustls with ring. Never a high-level HTTP client there.
- Country lookup is a direct HTTPS batch request, never through a proxy, and never blocks the table from rendering.

## Technologies

- Tauri 2 shell. Svelte 5 runes on plain Vite, no SvelteKit, no router. Plain CSS custom properties and scoped styles.
- No component kits, table libraries or CSS frameworks: native dialog, inputs and a small hand-written menu cover the UI.
- bun for JavaScript, cargo for Rust with the toolchain pinned in the repository. Never npm, yarn or pnpm.
- Minimum technologies: one tool per job. Adding a dependency needs the owner's approval; removing one is always welcome.
- Read version-matched official docs before using a framework API. Prefer documented seams over copied internals.

## Privacy

- Credentials never appear in the UI, logs, errors, notices or anything except exported files.
- Public issues, pull requests, commits, tests, release notes and docs never contain infrastructure details or account data.
- Describe failures generically, such as "the VPN resolver" or "a stuck subnet", never with addresses, subnet lists or credentials.
- Test fixtures use documentation address ranges and fake credentials only.

## Verification

- Tests cover the domain contract: parsing, grouping, health, scheduling, timings, inventory, settings and export. No UI tests unless asked.
- Tests live beside the code they cover, named after the behaviour.
- Agents verify UI with the Helium browser against the dev server on the fixture: drive the screen, read console errors, check every state.
- Then run the real app with `bun run tauri dev` and confirm launch, check, settings and import end to end.
- Before handing back, run `bun run check`, `bun run build`, cargo fmt check, clippy with warnings denied and the library tests. Report actual results.
- `bun run screenshot` refreshes the README screenshot from the fixture; it manages its own server and browser.
- Reread your diff and remove anything not needed.

## Working Safely

- Inspect Git status before editing. Preserve the owner's uncommitted changes; ask when ownership is unclear.
- Dedicated branch and pull request. Never reset, amend, rebase, force-push or discard work without authorization.
- Stage explicit paths only, never everything. Inspect the staged diff before committing.
- Conventional Commits: type, scope, imperative summary. Atomic commits, one purpose each.
- Ask the oracle agent to review the staged diff of risky changes before committing.
- Plans stay outside the repository and outside commits. Sub-agents never share files or delegate further.

## Release

- Bump the package manifest, the Tauri configuration and the Rust manifest to the same version, merge to main, then tag it.
- The release workflow builds signed, notarized macOS images for Apple Silicon and Intel and an unsigned Windows installer, plus updater metadata.
- Never replace published release assets; ship a higher patch version. Publishing needs the owner's go-ahead.

## Language

- English for identifiers, docs, commits and UI copy. No emoji and no horizontal rules in docs.

## Resources

- Tauri 2: https://v2.tauri.app/
- Tauri commands and events: https://v2.tauri.app/develop/calling-rust/
- Svelte 5 runes: https://svelte.dev/docs/svelte/what-are-runes
- Vite: https://vite.dev/guide/
- hyper client connections: https://docs.rs/hyper/latest/hyper/client/conn/http1/index.html
- rustls: https://docs.rs/rustls/latest/rustls/
- country.is API: https://country.is
- Simple design rules: https://martinfowler.com/bliki/BeckDesignRules.html
- YAGNI: https://martinfowler.com/bliki/Yagni.html
- Parse, do not validate: https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/
- SOLID: https://en.wikipedia.org/wiki/SOLID
- Conventional Commits: https://www.conventionalcommits.org/en/v1.0.0/
