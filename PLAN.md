# envy — Build Plan (Rust Core)

> Full idea/vision: see [idea-prompt.md](./idea-prompt.md).
> This document is the engineering plan for the Rust core engine.

## Goal

A single ultra-fast native binary (`envy`) that becomes the universal configuration layer
for every tech stack (Node.js, Python, Java/Spring, Go, Docker…). One file format
(`envy.yaml` schema + `envy.local.yaml` secrets), one command (`envy run <anything>`),
zero runtime dependencies for users.

## Non-Goals for MVP

- Cloud vault integrations (1Password / AWS / Vault) — Phase M5
- Hardware-backed encryption at rest — Phase M6
- Network-level leak blocking — Phase M8
- npm/pip/brew wrapper distribution — after core is stable (M7)

## Architecture

```
envy run <command>
      │
      ▼
┌──────────────┐   finds nearest    ┌───────────────┐
│ discovery.rs │──────────────────▶ │ envy.yaml     │ (schema, committed)
└──────────────┘                    │ envy.local.   │ (secrets, gitignored)
                                    └───────────────┘
      │
      ▼
┌──────────────┐   merge + validate types/formats
│ resolver.rs  │──────────────▶ errors / missing / warnings
└──────────────┘
      │ interactive TTY?
      ▼
┌──────────────┐  prompts for missing required keys,
│ prompt.rs    │  saves answers into envy.local.yaml
└──────────────┘
      │
      ▼
┌──────────────┐  spawns child process with injected env block
│ commands.rs  │──────▶ exit code propagated to shell (any stack works)
└──────────────┘
```

### Variable precedence (highest → lowest)

1. Real OS environment variable
2. `envy.local.yaml` values
3. `schema.default`
4. Interactive prompt (only if `required: true` and stdin is a TTY) → persisted to `envy.local.yaml`

### Validation matrix (MVP)

| type    | check                     |
|---------|---------------------------|
| string  | always ok                 |
| integer | parses as i64             |
| number  | parses as f64             |
| boolean | true/false/1/0/yes/no/on/off |

| format | check                                   |
|--------|------------------------------------------|
| uri/url  | has valid `scheme://`, no whitespace   |
| email    | local@domain.tld shape                 |
| uuid     | 8-4-4-4-12 hex groups                  |

Unknown keys present in `envy.local.yaml` but missing from the schema produce a typo warning.

## Repository Layout

```
Desktop/envy/
├── Cargo.toml
├── PLAN.md                ← this file
├── idea-prompt.md         ← original vision
└── src/
    ├── main.rs            entry point + exit-code handling
    ├── cli.rs             clap definitions (init/run/validate/list/setup)
    ├── schema.rs          envy.yaml model (VarSpec: type/format/required/secret/default)
    ├── local.rs           envy.local.yaml load/save (nested `values:` or flat form)
    ├── resolver.rs        merge + validation engine (pure, unit-tested)
    ├── prompt.rs          terminal prompting for missing required vars
    ├── discovery.rs       upward search + monorepo walker (skips node_modules/.git/…)
    ├── commands.rs        command implementations + process spawning
    └── template.envy.yaml scaffold used by `envy init`
```

## Command Surface (MVP)

| command              | behaviour                                                                 |
|----------------------|---------------------------------------------------------------------------|
| `envy init`          | writes commented `envy.yaml` template, adds `envy.local.yaml` to `.gitignore` |
| `envy run <cmd…>`    | resolve → prompt → validate → spawn child with injected env; propagates exit code |
| `envy validate`      | CI-friendly non-interactive check; exit 1 on any error/missing            |
| `envy list`          | pretty table of resolved values, secrets masked, `[source]` tags          |
| `envy setup --depth` | monorepo walk; per-service prompts; writes each service's local file      |

## Roadmap

- [x] **M0** Toolchain + repo scaffold (this commit)
- [x] **M1** Schema/local parsing, resolver with type+format validation, unit tests
- [x] **M2** CLI: init / run / validate / list / setup, process injection, exit codes
- [ ] **M3** Git-branch aware config hot-swapping (`envy.local.<branch>.yaml` overlay)
- [ ] **M4** Leak blocker pre-commit hook (`envy hook install`) scanning staged diffs
- [ ] **M5** Vault references in values (`op://…`, `aws://…`, `vault://…`) resolved in-memory
- [ ] **M6** Encryption-at-rest via OS keystore (TPM / Secure Enclave / Credential Manager)
- [ ] **M7** Release pipeline (CI + tag-triggered multi-platform release workflows done; npm/pip/gem/go/homebrew wrapper packages scaffolded in `packages/`) → remaining: first tagged release, publish wrappers, fill Homebrew sha256s
- [ ] **M8** Network egress guard for dev processes (blocklist of secret strings)
- [ ] **M9** Type-safe SDK generation (TS declarations, Go struct, Java class)
- [ ] **M10** `envy doctor` — did-you-mean suggestions for bad values/schemes

## Testing Strategy

- Unit tests inside `resolver.rs` and `local.rs` (pure logic: types, formats, parsing)
- End-to-end smoke test on a fixture project:
  - `envy validate` must fail on a malformed URI
  - must pass after fixing
  - `envy run cmd /C echo %PORT%` proves injection into a real child process
  - `envy list` masks secrets

## Performance Budget

- Cold start + resolve + spawn overhead target: **< 10 ms** (no network, no threads needed at MVP scale)
- Monorepo scan bounded by `--depth` and skip-list to stay instant on huge repos

## Security Notes (current state)

- `envy.local.yaml` is plain YAML today (Phase M6 adds encryption). It is never printed
  unmasked by `envy list`, and `envy init` always gitignores it.
- Prompted secret values are echoed once in the terminal (masked input comes with M6 UI work).
