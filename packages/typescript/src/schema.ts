export interface VarSpec {
  type?: string;
  format?: string;
  required?: boolean;
  secret?: boolean;
  mock?: boolean;
  description?: string;
  default?: unknown;
}

export interface SchemaFile {
  version: string;
  service?: string;
  config: Record<string, VarSpec>;
}

export type Source = "env" | "overlay" | "local" | "default" | "mock";

export interface LoadedConfig {
  service: string;
  /** Fully resolved values, ready for use. */
  values: Record<string, string>;
  /** Where each value came from. */
  sources: Record<string, Source>;
}

/**
 * Thrown when the configuration is invalid or incomplete.
 * Collects every problem at once so developers fix everything in one pass.
 */
export class EnvyError extends Error {
  public readonly problems: string[];

  constructor(problems: string[]) {
    super(
      `envy configuration invalid (${problems.length} problem(s)):\n` +
        problems.map((p) => `  - ${p}`).join("\n"),
    );
    this.name = "EnvyError";
    this.problems = problems;
  }
}
