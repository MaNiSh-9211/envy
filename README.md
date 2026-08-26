# envy

**Universal environment & configuration manager — one binary, every stack.**

Stop debugging broken `.env` files, `application.properties`, and missing keys.
`envy` gives your whole team (and your whole monorepo) a single, typed, validated
source of truth — then injects the values straight into **any** process:

```bash
envy run npm run dev
envy run ./mvnw spring-boot:run
envy run python main.py
envy run go run ./cmd/api
```

No code changes. No runtime dependencies. Sub-millisecond overhead.

## Why

Every developer knows this Slack message:

> "Hey, why is the backend failing?"
> "Oh, you need to add `NEXT_PUBLIC_NEW_API_KEY` to your local `.env`."

envy fixes that permanently: declare variables once per service, let envy validate,
prompt for what's missing, and boot anything with the right values.

## The two files

Every service uses exactly the same structure — Node.js or Spring Boot, Rust or Ruby:

**`envy.yaml`** — committed schema, single source of truth:

```yaml
version: "1"
service: payment-gateway

config:
  PORT:
    type: integer
    default: 8080
    description: Port the server binds to

  DATABASE_URL:
    type: string
    format: uri
    required: true

  STRIPE_SECRET_KEY:
    type: string
    secret: true
    required: true

  FEATURE_FLAG_NEW_UI:
    type: boolean
    default: false
```

**`envy.local.yaml`** — gitignored local secrets:

```yaml
values:
  DATABASE_URL: "postgresql://postgres:local@localhost:5432/db"
  STRIPE_SECRET_KEY: "sk_test_51Nx..."
```

## Install

| Runtime | Command |
|---|---|
| npm (CLI) | `npm install -g envy-cli` |
| pip | `pip install envy-cli` |
| RubyGems | `gem install envy-cli` |
| Go | `go run github.com/MaNiSh-9211/envy/packages/go/cmd/envy-installer@latest` |
| Homebrew | formula in [`packages/homebrew`](packages/homebrew) after first release |
| Cargo (core) | `cargo install --git https://github.com/MaNiSh-9211/envy envy` |
| Shell (mac/Linux) | `curl -fsSL https://raw.githubusercontent.com/MaNiSh-9211/envy/main/scripts/install.sh \| sh` |
| PowerShell (Windows) | see [`scripts/install.ps1`](scripts/install.ps1) |

### Language libraries (no CLI required)

Load and validate `envy.yaml` natively inside your app:

| Library | Install | Import |
|---|---|---|
| Rust (core crate) | `cargo add envy` | `envy::resolver::resolve(&schema, &layers, &opts)` |
| TypeScript / Node | `npm install @envy/config` | [`loadConfig()`](packages/typescript#readme) — fully typed, throws `EnvyError` with all problems |
| Java / JVM / Spring | `io.github.manish-9211:envy-java` | [`Envy.load()`](packages/maven#readme) — Map<String,String>, cached |
| PHP 8.1+ | `composer require manish-9211/envy-php` | [`Envy::load()`](packages/php#readme) — array, cached |
| .NET / C# | `dotnet add package Envy.Config` | [`EnvyLoader.Load()`](packages/dotnet#readme) — IReadOnlyDictionary |

All of them share the exact same precedence, validation and mock semantics as
the core Rust engine.

## Commands

| command | behaviour |
|---|---|
| `envy init` | scaffold an `envy.yaml` + gitignore `envy.local.yaml` |
| `envy run [--guard] <cmd…>` | resolve → prompt → validate → spawn with injected env; `--guard` kills the process if a secret leaks into output (exit 2) |
| `envy validate` | CI-friendly non-interactive check |
| `envy list` | resolved table, secrets masked, `[source]` tags |
| `envy setup` | scan a monorepo, fill missing values for every service sequentially |
| `envy diff <env>` | compare local vs `envy.<env>.yaml` — secrets masked, mismatches highlighted |
| `envy doctor` | explains every problem and offers one-keystroke auto-fixes (`postgersql://` → `postgresql://`) |
| `envy lock` / `envy unlock` | AES-256-GCM encrypt `envy.local.yaml` at rest, key held in the OS keystore |
| `envy hook install` | pre-commit hook that blocks commits containing your declared secrets |
| `envy scan [--all]` | leak-scan staged changes (or the whole tree) |
| `envy gen <ts\|go\|java\|python>` | generate type-safe config bindings → autocomplete, no typos |

### Variable precedence

1. Real OS environment variable
2. **Branch overlay** — `envy.local.<branch>.yaml` (git-branch aware hot-swapping)
3. `envy.local.yaml`
4. Schema default
5. **Generated mock** when the schema says `mock: true`
6. Interactive prompt (required vars) — answers saved locally

### Secrets that never touch disk

Values can be live vault references resolved in-memory at boot:

```yaml
values:
  API_SECRET: "op://dev-vault/stripe/key"        # 1Password CLI
  DB_PASSWORD: "vault://secret/data/db#password" # Vault CLI
  STRIPE_KEY: "aws://prod/stripe#api_key"        # AWS Secrets Manager
```

Run `--offline` to skip resolution on a plane.

### Encryption at rest

```bash
envy lock     # envy.local.yaml becomes ciphertext; key lives in
              # Windows Credential Manager / macOS Keychain / Secret Service
envy unlock   # back to plaintext for manual editing
```

AES-256-GCM. Decryption happens in memory only, for milliseconds per boot.

### Validation built in

- types: `integer`, `number`, `boolean`, `string`
- formats: `uri`/`url`, `email`, `uuid`
- unknown local keys warn you about likely typos
- required-but-missing keys stop the boot before your app crashes cryptically

## How it works across stacks

| Stack | What envy does | Your code reads |
|---|---|---|
| Node.js | injects into child process env block | `process.env.PORT` |
| Python / Go / Ruby / Rust | same env injection | `os.environ`, `os.Getenv`, `ENV[...]` |
| Java / Spring Boot | env vars visible to the JVM | `System.getenv("PORT")`, `${PORT}` |
| Docker | generate `.env.docker` on demand | standard container intake |

## Monorepo workflow

```bash
git clone your-company/mono
cd mono
envy setup        # walks every service/, prompts once for each missing key
envy run ./mvnw spring-boot:run   # any stack, same command shape
```

## Roadmap

See [PLAN.md](PLAN.md): branch-aware config hot-swapping, vault references
(`op://`, `aws://`, `vault://`), encryption-at-rest via TPM/Secure Enclave,
leak-blocking pre-commit hook, and type-safe SDK generation.

## License

MIT — see [LICENSE](LICENSE).
