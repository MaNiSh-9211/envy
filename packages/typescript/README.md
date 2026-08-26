# @envy/config (TypeScript / JavaScript)

Typed, validated configuration loading for `envy.yaml` **as a library** — no CLI
required at runtime. Mirrors the exact semantics of the envy core engine:
upward search, git-branch overlays, precedence layers, type/format validation,
did-you-mean suggestions, and deterministic mocks.

## Install

```bash
npm install @envy/config
```

## Usage

```ts
import { loadConfig } from "@envy/config";

const config = loadConfig(); // searches upward from process.cwd()

console.log(config.service);            // "payment-gateway"
console.log(config.values.PORT);        // "8080"
console.log(config.sources.DATABASE_URL); // "local" | "env" | "overlay" | ...
```

Combine with the generated declarations for full autocomplete:

```bash
envy gen typescript   # emits envy.d.ts
```

```ts
process.env.DATABASE_URL; // fully typed via envy.d.ts
```

## Behaviour

| aspect | detail |
|---|---|
| precedence | `process.env` → branch overlay → `envy.local.yaml` → default → generated mock |
| validation | integer / number / boolean + `uri`, `email`, `uuid` formats |
| suggestions | typo'd keys and schemes come back with "did you mean …?" |
| failure mode | one `EnvyError` collecting **every** problem in a single pass |
| secrets | never logged; values only live in memory |

## Error handling

```ts
import { EnvyError } from "@envy/config";

try {
  const config = loadConfig();
} catch (err) {
  if (err instanceof EnvyError) {
    for (const problem of err.problems) console.error(problem);
  }
}
```
