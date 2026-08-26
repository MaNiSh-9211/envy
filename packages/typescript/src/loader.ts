import * as fs from "node:fs";
import * as path from "node:path";
import { execFileSync } from "node:child_process";
import { parse as parseYaml } from "yaml";
import { EnvyError, LoadedConfig, SchemaFile, Source, VarSpec } from "./schema";

const SCHEMA_FILE = "envy.yaml";

type Values = Record<string, unknown>;

function findUpward(name: string, from: string): string | null {
  let dir = path.resolve(from);
  for (;;) {
    const candidate = path.join(dir, name);
    if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
      return candidate;
    }
    const parent = path.dirname(dir);
    if (parent === dir) {
      return null;
    }
    dir = parent;
  }
}

function readYaml<T>(file: string): T | null {
  const text = fs.readFileSync(file, "utf8");
  if (!text.trim()) {
    return null;
  }
  try {
    return parseYaml(text) as T;
  } catch (err) {
    const detail = err instanceof Error ? err.message.split("\n")[0] : String(err);
    throw new EnvyError([`${path.basename(file)}: ${detail}`]);
  }
}

function valuesFrom(doc: unknown, label: string): Values {
  if (doc === null || doc === undefined) {
    return {};
  }
  if (typeof doc !== "object" || Array.isArray(doc)) {
    throw new EnvyError([`${label}: expected a mapping`]);
  }
  const map = doc as Record<string, unknown>;
  const inner = map.values;
  if (inner !== undefined && typeof inner === "object" && !Array.isArray(inner)) {
    return inner as Values;
  }
  return map;
}

export function currentBranch(from: string): string | null {
  try {
    const out = execFileSync(
      "git",
      ["-C", from, "rev-parse", "--abbrev-ref", "HEAD"],
      { stdio: ["ignore", "pipe", "ignore"], timeout: 2000 },
    );
    const branch = out.toString().trim();
    return branch && branch !== "HEAD" ? branch : null;
  } catch {
    return null;
  }
}

export function sanitizeBranch(branch: string): string {
  return branch.replace(/[^A-Za-z0-9._-]/g, "-");
}

function scalarToString(value: unknown): string | null {
  if (value === null || value === undefined) {
    return null;
  }
  if (typeof value === "string") {
    return value;
  }
  if (typeof value === "boolean" || typeof value === "number") {
    return String(value);
  }
  return null;
}

const URI_SCHEMES = [
  "postgresql", "postgres", "mysql", "mariadb", "mssql", "mongodb", "mongodb+srv",
  "redis", "rediss", "amqp", "rabbitmq", "kafka", "http", "https", "ws", "wss",
  "grpc", "ftp", "sftp", "ssh", "smtp", "s3", "gs", "azblob", "sqlite",
];

function levenshtein(a: string, b: string): number {
  const dp: number[][] = Array.from({ length: a.length + 1 }, (_, i) => [i]);
  for (let j = 0; j <= b.length; j++) {
    dp[0][j] = j;
  }
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      dp[i][j] = Math.min(dp[i - 1][j] + 1, dp[i][j - 1] + 1, dp[i - 1][j - 1] + cost);
      if (
        i > 1 && j > 1 &&
        a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]
      ) {
        dp[i][j] = Math.min(dp[i][j], dp[i - 2][j - 2] + 1);
      }
    }
  }
  return dp[a.length][b.length];
}

function schemeHint(raw: string): string | null {
  const idx = raw.indexOf("://");
  if (idx <= 0) {
    return null;
  }
  const scheme = raw.slice(0, idx);
  if (URI_SCHEMES.includes(scheme)) {
    return null;
  }
  let best: { name: string; distance: number } | null = null;
  for (const candidate of URI_SCHEMES) {
    const distance = levenshtein(scheme, candidate);
    if (distance <= Math.max(1, Math.min(3, Math.floor(scheme.length / 3)))) {
      if (!best || distance < best.distance) {
        best = { name: candidate, distance };
      }
    }
  }
  return best ? `${best.name}://${raw.slice(idx + 3)}` : null;
}

function checkType(spec: VarSpec, key: string, raw: string): string | null {
  const trimmed = raw.trim();
  switch (spec.type ?? "string") {
    case "integer":
      return /^-?\d+$/.test(trimmed) ? null : `${key}: expected an integer, got "${raw}"`;
    case "number":
    case "float":
      return Number.isFinite(Number(trimmed)) ? null : `${key}: expected a number, got "${raw}"`;
    case "boolean":
    case "bool":
      return ["true", "false", "1", "0", "yes", "no", "on", "off"].includes(trimmed.toLowerCase())
        ? null
        : `${key}: expected a boolean (true/false), got "${raw}"`;
    default:
      return null;
  }
}

function checkFormat(spec: VarSpec, key: string, raw: string): string | null {
  switch (spec.format) {
    case "uri":
    case "url": {
      const match = /^([A-Za-z0-9+.-]+):\/\/(\S*)$/.exec(raw);
      if (!match) {
        const hint = schemeHint(raw);
        return hint
          ? `${key}: does not satisfy format '${spec.format}' — did you mean ${hint}?`
          : `${key}: does not satisfy format '${spec.format}': "${raw}"`;
      }
      const [, scheme] = match;
      if (!URI_SCHEMES.includes(scheme)) {
        const hint = schemeHint(raw);
        if (hint) {
          return `${key}: does not satisfy format '${spec.format}' — did you mean ${hint}?`;
        }
      }
      return null;
    }
    case "email":
      return /^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(raw)
        ? null
        : `${key}: does not satisfy format 'email': "${raw}"`;
    case "uuid":
      return /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(raw)
        ? null
        : `${key}: does not satisfy format 'uuid': "${raw}"`;
    default:
      return null;
  }
}

function mockValue(key: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < key.length; i++) {
    hash ^= key.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  const hex = hash.toString(16).padStart(8, "0").repeat(4);
  return `mock_${hex}`;
}

/**
 * Locate, layer and validate configuration starting from `from`.
 *
 * Precedence (highest wins):
 *   process.env → envy.local.<branch>.yaml → envy.local.yaml → schema default → generated mock
 *
 * Throws {@link EnvyError} collecting every problem in one pass.
 */
export function loadConfig(from: string = process.cwd()): LoadedConfig {
  const problems: string[] = [];
  const schemaPath = findUpward(SCHEMA_FILE, from);
  if (!schemaPath) {
    throw new EnvyError([
      `no ${SCHEMA_FILE} found in ${from} or any parent directory — run \`envy init\` first`,
    ]);
  }

  const schemaDoc = readYaml<Partial<SchemaFile>>(schemaPath);
  if (!schemaDoc || typeof schemaDoc !== "object") {
    throw new EnvyError([`${SCHEMA_FILE}: empty or invalid schema`]);
  }
  const config = (schemaDoc.config ?? {}) as Record<string, VarSpec>;

  const baseDir = path.dirname(schemaPath);
  const branch = currentBranch(baseDir);
  const overlayFile = branch
    ? path.join(baseDir, `envy.local.${sanitizeBranch(branch)}.yaml`)
    : null;
  const localFile = path.join(baseDir, "envy.local.yaml");

  const overlay: Values =
    overlayFile && fs.existsSync(overlayFile)
      ? valuesFrom(readYaml<unknown>(overlayFile), path.basename(overlayFile))
      : {};
  const local: Values = fs.existsSync(localFile)
    ? valuesFrom(readYaml<unknown>(localFile), path.basename(localFile))
    : {};

  const values: Record<string, string> = {};
  const sources: Record<string, Source> = {};

  for (const [key, spec] of Object.entries(config)) {
    let placed: { value: string; source: Source } | null = null;

    if (process.env[key] !== undefined) {
      placed = { value: process.env[key] as string, source: "env" };
    } else if (key in overlay) {
      const scalar = scalarToString(overlay[key]);
      if (scalar === null) {
        problems.push(`${key}: expected a scalar in the branch overlay`);
      } else {
        placed = { value: scalar, source: "overlay" };
      }
    } else if (key in local) {
      const scalar = scalarToString(local[key]);
      if (scalar === null) {
        problems.push(`${key}: expected a scalar in envy.local.yaml`);
      } else {
        placed = { value: scalar, source: "local" };
      }
    } else if (spec.default !== undefined) {
      const scalar = scalarToString(spec.default);
      if (scalar === null) {
        problems.push(`${key}: bad default — must be a scalar`);
      } else {
        placed = { value: scalar, source: "default" };
      }
    } else if (spec.mock) {
      const mocked = mockValue(key);
      placed = { value: mocked, source: "mock" };
    }

    if (!placed) {
      if (spec.required) {
        problems.push(`missing required variable ${key}`);
      }
      continue;
    }

    const typeProblem = checkType(spec, key, placed.value);
    if (typeProblem) {
      problems.push(typeProblem);
    }
    const formatProblem = checkFormat(spec, key, placed.value);
    if (formatProblem) {
      problems.push(formatProblem);
    }

    values[key] = placed.value;
    sources[key] = placed.source;
  }

  const knownKeys = new Set(Object.keys(config));
  for (const [layerName, layer] of [
    ["branch overlay", overlay],
    ["envy.local.yaml", local],
  ] as Array<[string, Values]>) {
    for (const key of Object.keys(layer)) {
      if (!knownKeys.has(key)) {
        let best: { name: string; distance: number } | null = null;
        for (const candidate of knownKeys) {
          const distance = levenshtein(key, candidate);
          if (distance <= Math.max(1, Math.min(3, Math.floor(key.length / 3)))) {
            if (!best || distance < best.distance) {
              best = { name: candidate, distance };
            }
          }
        }
        const hint = best ? ` — did you mean ${best.name}?` : "";
        problems.push(`${key} is set in ${layerName} but not declared in ${SCHEMA_FILE} (typo?)${hint}`);
      }
    }
  }

  if (problems.length > 0) {
    throw new EnvyError(problems);
  }

  return {
    service: schemaDoc.service ?? "unnamed-service",
    values,
    sources,
  };
}
