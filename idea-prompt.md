# envy — Universal Configuration Manager (Build in Rust)

## The Idea: zero-config-env / env-sync

If we are talking about absolute, instant viral growth for a highly useful package, it has to solve a modern, agonizing pain point that every developer is facing right now.

That package is zero-config-env (or env-sync). It solves the "broken environment variables" nightmare that happens every time a developer pulls code, switches git branches, or onboards a new team member.

### The Problem It Solves

We have all seen the slack message: "Hey, why is the backend failing?" and the response is always: "Oh, you need to add NEXT_PUBLIC_NEW_API_KEY to your local .env file."

Current tools like dotenv only read files. They don't validate them, sync them across teams safely, or let you know when your local configuration is missing a newly added variable from a teammate.

### How zero-config-env Works

It is a zero-dependency, ultra-fast wrapper around environment variables that automates local setup.

- **Smart Tracking**: It automatically tracks changes to your `.env.example` file.
- **Auto-Prompt**: When a developer runs `npm run dev`, it instantly scans their `.env` file. If a new variable was added to the repo by someone else, the terminal pauses and prompts: `"Variable STRIPE_SECRET_KEY is missing. Paste it here to update your .env:"`
- **Type-Safe Validation**: It automatically infers types. If a variable should be a number (like `PORT=3000`), it throws a clear terminal error if someone types `PORT=three-thousand`.
- **Zero Leak Guarantee**: It scans your code before git commits. If it detects you accidentally hardcoded an API key in a component instead of using the `.env` file, it blocks the commit and points to the exact line.

### Why It Goes Viral

- **Universal Pain**: Every single developer—frontend, backend, fullstack—deals with `.env` issues.
- **Zero Onboarding Friction**: You run `npx zero-config-env init` once, and your entire team's environment variables are automatically kept in sync via local encrypted diffs.
- **The "Aha!" Moment**: The first time a developer pulls down a broken branch, and the package automatically fixes their config instead of letting the app crash, they will instantly share it on X/Twitter and GitHub.

---

## Making It Universal Across Tech Stacks

To make this truly universal and work across Node.js, Spring Boot, Python, Go, and Ruby, we cannot rely on a standard npm library. A Python developer won't install an npm package just to manage their configuration.

Instead, the solution is a **Universal CLI Binary** written in Go or Rust, distributed via package managers like npm, brew, pip, and cargo.

Let's call this universal tool **envy**.

### How a Universal Solution Works

Instead of reading specific languages, envy acts as a wrapper that injects configuration into any command you run, regardless of the tech stack.

Instead of running:

- `npm run dev` (Node)
- `./mvnw spring-boot:run` (Java/Spring)
- `python main.py` (Python)

Developers run:

- `envy run npm run dev`
- `envy run ./mvnw spring-boot:run`

### Core Mechanism: Multi-Format Translation

envy maintains a single, encrypted source of truth in the project root (e.g., `.envy.json` or `.envy.yaml`).

When a developer runs `envy run [command]`, the CLI instantly detects the project type and auto-generates or injects the native config files required by that specific framework on the fly:

- For **Spring Boot**: It generates `application.properties` or `application.yml`.
- For **Node.js / Python**: It injects standard process environment variables directly into the runtime memory, or writes a temporary `.env` file.
- For **Docker**: It can output an `.env.docker` file.

### Key Universal Features

- **Cross-Language Schema Validation**: You define your configuration schema once (e.g., PORT must be an integer, DATABASE_URL must be a valid URI). envy validates this before the app even attempts to boot, stopping runtime crashes in any language.
- **Global Team Syncing**: It safely hooks into your team's central vault (like 1Password, AWS Secrets Manager, or an encrypted git file) to pull down missing variables seamlessly.
- **The Wrapper Distribution Trick**: To make it viral across communities, you publish the exact same tool to every package registry:
  - Node devs: `npm install -g envy-cli`
  - Python devs: `pip install envy-cli`
  - Mac devs: `brew install envy`

### Why This Goes Hyper-Viral

This crosses ecosystem boundaries. A frontend engineer working in Node.js and a backend engineer working in Spring Boot can use the exact same tool, share the same configuration schema, and onboarding to a polyglot microservice repository takes literally one command: `envy setup`.

---

## Monorepo Support: One Command, Same File Everywhere

A mono repo having different services in different tech stacks — only one command is enough there, and it has to be really fast, support all platforms where needed, and across tech stacks/runtimes the file name and structure should be same.

To build a blazing-fast, universal monorepo configuration manager (envy), you only need exactly one unified schema format across the entire repository. This completely replaces standard `.env` files, Java `application.properties`, and Python config scripts.

To guarantee maximum speed and effortless cross-platform support, the file name and structure must be strictly uniform.

### 1. The File Names

You only need exactly two files per microservice/app in the monorepo:

- **`envy.yaml`**: The single source of truth configuration schema. It is tracked by Git, completely public, and maps out what configuration variables are required, their types, and descriptions.
- **`envy.local.yaml`**: The local-only override file. It is explicitly added to `.gitignore` and houses secret developer credentials (like private API keys or custom local database passwords).

### 2. The Universal Structure

By leveraging YAML, you gain an explicit, human-readable structure that easily compiles into JSON, plain environment strings, or Java properties under the hood.

#### The Global/App Schema (`envy.yaml`)

Every microservice (Node.js, Spring Boot, Python, Go) uses this exact structure:

```yaml
version: "1"
service: "payment-gateway" # Name of the microservice

config:
  PORT:
    type: "integer"
    default: 8080
    description: "The port the application server binds to"

  DATABASE_URL:
    type: "string"
    format: "uri"
    required: true
    description: "Primary database connection string"

  STRIPE_SECRET_KEY:
    type: "string"
    secret: true # Tells envy to look for this in envy.local.yaml or secret managers
    required: true

  FEATURE_FLAG_NEW_UI:
    type: "boolean"
    default: false
```

#### The Developer's Secrets File (`envy.local.yaml`)

This file is structurally matching but contains only the specific real values that should never be checked into Git:

```yaml
values:
  DATABASE_URL: "postgresql://postgres:localdevpass@localhost:5432/db"
  STRIPE_SECRET_KEY: "sk_test_51Nx..."
  FEATURE_FLAG_NEW_UI: true # Overriding the schema default locally
```

### 3. How envy Handles Different Tech Stacks with One Command

When a developer runs `envy run npm run dev` inside a Node.js folder, or `envy run ./mvnw spring-boot:run` in a Java folder, the Rust/Go binary handles the translation behind the scenes in milliseconds:

| Target Tech Stack | How envy Injects the Config | Developer Code Experience |
|---|---|---|
| Node.js | Injects directly into system memory (`process.env`). | `const port = process.env.PORT;` |
| Python / Go | Injects directly into the process environment block. | `os.environ.get('PORT')` / `os.Getenv("PORT")` |
| Spring Boot (Java) | Automatically translates flat key-value pairs into standard JVM system properties on the fly. | `@Value("${PORT}")` or `System.getenv("PORT")` |
| Docker | Spits out a temporary `.env.docker` dynamically if needed. | Standard container environmental intake. |

### Why this specific setup makes it viral

- **Zero structural differences**: Whether a developer is checking out a Rust microservice or a legacy Java service, they look for `envy.yaml`.
- **Instant Onboarding**: A new developer runs one global command at the root of the monorepo: `envy setup`. It scans all subfolders, identifies missing keys in every `envy.local.yaml`, and prompts them to fill them out sequentially.

---

## Futuristic Feature List (Make Devs Ditch Everything Else)

Features that feel like they are from the year 2030. It shouldn't just store configuration; it should actively manage, heal, and secure it.

### 1. Zero-Trust Local Encryption (Crypt-at-Rest)
Traditional local files like `envy.local.yaml` are stored in plain text, making them a massive security risk if a developer's laptop is stolen or compromised. envy encrypts `envy.local.yaml` at rest using hardware-backed encryption (like Apple Secure Enclave or Windows TPM). Secrets are only decrypted in-memory for a fraction of a millisecond when `envy run` executes. Your local environment variables are as secure as a production vault.

### 2. "Git-Branch Aware" Config Hot-Swapping
When working in a monorepo, switching from a main branch to a feature branch often requires completely changing feature flags, API keys, or database URLs. envy tracks your current Git branch. When you run `git checkout feature-xyz`, envy instantly and automatically swaps out your local configurations to match that exact feature branch's requirements without you lifting a finger.

### 3. Dynamic Local Secret Sourcing (The Vault Wrapper)
Instead of copy-pasting values from 1Password, Bitwarden, AWS Secrets Manager, or HashiCorp Vault into a local file, envy fetches them dynamically. In your `envy.local.yaml`, you simply write `STRIPE_SECRET_KEY: "op://dev-vault/stripe/credential"`. When the app boots, envy securely pulls the fresh key directly from your password manager into system memory. No secrets are ever written to disk.

### 4. Smart Auto-Healing & Self-Mocking
If a third-party service (like Twilio or SendGrid) is down, or if a developer doesn't have a paid API key for local testing, envy can spin up a local mock server on the fly. You can configure the schema to say `mock: true`. If envy detects the key is missing or invalid, it automatically mocks the API responses locally so the developer can keep coding offline without an account.

### 5. Live Environment Diffing & Telemetry
If a service breaks, developers waste hours checking if their local setup matches staging or production environments. Running `envy diff staging` instantly compares your local values against staging, highlighting mismatched values or outdated configurations in a clean terminal side-by-side view.

### 6. AI-Powered Configuration Dr. (Self-Correction)
An embedded, ultra-lightweight, offline AI model analyzes configuration errors. If your database connection string fails because you misspelled `postgresql://` as `postgre://`, or forgot a slash, envy catches it. Instead of a cryptic stack trace crashing your Spring Boot or Node app, envy intercepts the crash and outputs: "Hey, your DATABASE_URL format is slightly off. Did you mean postgresql://...? Press Enter to auto-fix."

### 7. Compile-Time Type Guarantee For Any Language
envy generates native, type-safe configuration SDKs or bindings for your specific programming languages on the fly before running the code:
- For TypeScript/Node, it generates a strictly typed `process.env` global declaration.
- For Go, it generates a native struct.
- For Java/Spring, it generates a type-validated configuration class.

You get autocomplete in VS Code/IntelliJ for your configuration keys, making typos physically impossible.

### 8. Strict Leak Blocker (Pre-Commit & Network Level)
It doesn't just act as a standard pre-commit git hook. envy profiles your application's outgoing network requests during local development. If your application accidentally leaks a raw secret string over an unencrypted HTTP log or sends a private key to a third-party analytics tracker, envy immediately terminates the process and flags the rogue line of code.

---

## What Language to Build In: Rust

Do not use Node.js, Python, or Java to build the main tool. If you do, users will have to install those specific runtimes just to use your tool, which completely destroys the "universal" promise.

**Decision: Build in Rust.**

Why Rust:

- **Single, Tiny Binary**: Rust compiles down to a single, highly optimized executable file with zero external dependencies.
- **Instant Startup Time (Sub-millisecond)**: Since envy must wrap other commands (e.g., `envy run npm run dev`), it cannot add any noticeable overhead. Rust starts up in microseconds.
- **Hardware Security (TPM/Secure Enclave)**: Rust has mature, low-level crates to talk directly to your computer's hardware for the Zero-Trust Encryption feature.
- **Memory Safety**: Rust prevents crashes and memory leaks out of the box, ensuring the CLI tool never breaks a developer's workflow.
- **Maximum hype/virality** in the open-source community (Vercel, Supabase, Biome are rewriting tools in Rust).

### The Hybrid Distribution Strategy

While the core engine is written in Rust, publish lightweight wrapper packages to every package registry:

1. Write the engine in Rust.
2. Compile it for Mac, Windows, and Linux.
3. Publish a tiny npm package (`envy-cli`) that simply downloads the correct binary for the user's system.
4. Do the exact same thing for pip (Python) and Homebrew (Mac/Linux).

A Node.js dev types `npm install envy`, a Python dev types `pip install envy`, but they are both secretly running the exact same hyper-fast Rust binary under the hood.

```
                              ┌──► [npm package] ──► (Downloads Binary) ──► Node.js Devs
                              │
[Core Engine in Rust] ────────┼──► [pip package] ──► (Downloads Binary) ──► Python Devs
                              │
                              └──► [Homebrew Formula] ────────────────────► Mac/Linux Devs
```

### How the Universal Execution Works (No Code Changes Needed)

Because the core engine is a compiled binary, it can manipulate system processes directly. It doesn't care what language the user's application is written in.

**Example 1: Node.js Project**

```bash
envy run npm run dev
```

1. envy boots up instantly (under 1 millisecond).
2. Reads `envy.yaml` and validates the keys.
3. Spawns `npm run dev` as a child process.
4. Injects the keys directly into that child process's environment memory block.
5. Node.js natively reads them via `process.env`.

**Example 2: Java Spring Boot Project**

```bash
envy run ./mvnw spring-boot:run
```

1. envy boots up instantly.
2. Reads the exact same `envy.yaml`.
3. Spawns the Java process.
4. Passes the keys as standard environment variables or system properties directly to the JVM.
5. Spring Boot natively reads them via `@Value("${KEY}")` or `System.getenv()`.

---

## Step-by-Step Roadmap

1. Pick Rust and build the standalone CLI binary first. Make it work locally by running `./envy run echo "hello"`.
2. Add the translation logic so it reads YAML and passes those variables down to whatever command follows `envy run`.
3. Set up a GitHub Action to automatically compile Windows, Mac, and Linux binaries whenever code is pushed.
4. Create the wrapper scripts for npm, pip, and Homebrew so developers can install it using their favorite tools.
