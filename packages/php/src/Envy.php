<?php

declare(strict_types=1);

namespace Envy;

use Symfony\Component\Yaml\Yaml;

/**
 * Loads and validates {@code envy.yaml} natively in PHP — Laravel, Symfony,
 * plain scripts, anything. Mirrors the precedence of the envy core engine:
 *
 *   putenv/env → envy.local.<branch>.yaml → envy.local.yaml → schema default → generated mock
 *
 * Usage:
 *   $config = \Envy\Envy::load();
 *   $dbUrl  = $config['DATABASE_URL'];
 */
final class Envy
{
    private const SCHEMA_FILE = 'envy.yaml';

    private const KNOWN_SCHEMES = [
        'postgresql', 'postgres', 'mysql', 'mariadb', 'mssql', 'mongodb',
        'mongodb+srv', 'redis', 'rediss', 'amqp', 'rabbitmq', 'kafka',
        'http', 'https', 'ws', 'wss', 'grpc', 'ftp', 'sftp', 'ssh',
        'smtp', 's3', 'gs', 'azblob', 'sqlite',
    ];

    /** @var array<string, array<string, string>>|null */
    private static ?array $cache = null;

    private function __construct()
    {
    }

    /** Load from the current working directory (searching upward). */
    public static function load(): array
    {
        return self::loadFrom(getcwd() ?: '.');
    }

    /**
     * Load searching upward from $startDir. Cached per schema location.
     *
     * @return array<string, string> resolved variable => value
     */
    public static function loadFrom(string $startDir): array
    {
        $schemaPath = self::findUpward(self::SCHEMA_FILE, self::normalize($startDir));
        if ($schemaPath === null) {
            throw new EnvyException([
                sprintf('no %s found in %s or any parent directory — run `envy init` first', self::SCHEMA_FILE, $startDir),
            ]);
        }

        if (self::$cache !== null && isset(self::$cache[$schemaPath])) {
            return self::$cache[$schemaPath];
        }

        $problems = [];
        $schemaDoc = self::readYaml($schemaPath, $problems);
        if ($schemaDoc === null || !is_array($schemaDoc)) {
            throw new EnvyException([$schemaPath.': empty or invalid schema']);
        }

        /** @var array<string, array<string, mixed>> $config */
        $config = is_array($schemaDoc['config'] ?? null) ? $schemaDoc['config'] : [];

        $baseDir = dirname($schemaPath);
        $branch = self::currentBranch($baseDir);
        $overlayFile = $branch !== null
            ? $baseDir.DIRECTORY_SEPARATOR.'envy.local.'.self::sanitizeBranch($branch).'.yaml'
            : null;
        $localFile = $baseDir.DIRECTORY_SEPARATOR.'envy.local.yaml';

        $overlay = ($overlayFile !== null && is_file($overlayFile))
            ? self::valuesOf(self::readYaml($overlayFile, $problems), basename($overlayFile), $problems)
            : [];
        $local = is_file($localFile)
            ? self::valuesOf(self::readYaml($localFile, $problems), 'envy.local.yaml', $problems)
            : [];

        $resolved = [];

        foreach ($config as $key => $spec) {
            $spec = is_array($spec) ? $spec : [];
            $placed = null;
            $value = null;

            $envValue = getenv($key);
            if ($envValue !== false && $envValue !== '') {
                $value = $envValue;
                $placed = true;
            } elseif (array_key_exists($key, $overlay)) {
                $value = self::scalarToString($overlay[$key], $key, $problems);
                $placed = $value !== null;
            } elseif (array_key_exists($key, $local)) {
                $value = self::scalarToString($local[$key], $key, $problems);
                $placed = $value !== null;
            } elseif (array_key_exists('default', $spec) && $spec['default'] !== null) {
                $value = self::scalarToString($spec['default'], $key, $problems);
                $placed = $value !== null;
            } elseif (!empty($spec['mock'])) {
                $value = self::mockValue($key);
                $placed = true;
            }

            if (!$placed || $value === null) {
                if (!empty($spec['required'])) {
                    $problems[] = 'missing required variable '.$key;
                }
                continue;
            }

            self::checkType($spec, $key, $value, $problems);
            self::checkFormat($spec, $key, $value, $problems);
            $resolved[$key] = $value;
        }

        foreach (['branch overlay' => $overlay, 'envy.local.yaml' => $local] as $label => $layer) {
            foreach (array_keys($layer) as $key) {
                if (!array_key_exists($key, $config)) {
                    $suggestion = self::bestKeyMatch((string) $key, array_keys($config));
                    $problems[] = $key.' is set in '.$label.' but not declared in '
                        .self::SCHEMA_FILE.' (typo?)'
                        .($suggestion !== null ? ' — did you mean '.$suggestion.'?' : '');
                }
            }
        }

        if ($problems !== []) {
            throw new EnvyException($problems);
        }

        self::$cache[$schemaPath] = $resolved;

        return $resolved;
    }

    /** Convenience accessor over load(). */
    public static function get(string $key): ?string
    {
        return self::load()[$key] ?? null;
    }

    // ---------------------------------------------------------------- internals

    private static function normalize(string $dir): string
    {
        $real = realpath($dir);

        return $real === false ? rtrim($dir, '/\\') : $real;
    }

    private static function findUpward(string $name, string $start): ?string
    {
        $dir = $start;
        for (;;) {
            $candidate = $dir.DIRECTORY_SEPARATOR.$name;
            if (is_file($candidate)) {
                return $candidate;
            }
            $parent = dirname($dir);
            if ($parent === $dir) {
                return null;
            }
            $dir = $parent;
        }
    }

    private static function readYaml(string $file, array &$problems): mixed
    {
        try {
            $text = file_get_contents($file);
            if ($text === false || trim((string) $text) === '') {
                return null;
            }

            return Yaml::parse($text);
        } catch (\Throwable $e) {
            $problems[] = basename($file).': '.$e->getMessage();

            return null;
        }
    }

    /**
     * Accepts the nested `values:` form or a flat SCREAMING_SNAKE mapping.
     *
     * @return array<string, mixed>
     */
    private static function valuesOf(mixed $doc, string $label, array &$problems): array
    {
        if ($doc === null) {
            return [];
        }
        if (!is_array($doc)) {
            $problems[] = $label.': expected a mapping';

            return [];
        }
        if (isset($doc['values']) && is_array($doc['values'])) {
            /** @var array<string, mixed> $values */
            $values = $doc['values'];

            return $values;
        }
        unset($doc['service'], $doc['version']);
        foreach (array_keys($doc) as $key) {
            if (!is_string($key) || $key !== strtoupper($key) || str_contains($key, ':')) {
                $problems[] = $label.': expected a `values:` mapping or flat KEY: value pairs';

                return [];
            }
        }

        /** @var array<string, mixed> $doc */
        return $doc;
    }

    private static function scalarToString(mixed $value, string $key, array &$problems): ?string
    {
        if (is_string($value)) {
            return $value;
        }
        if (is_bool($value)) {
            return $value ? 'true' : 'false';
        }
        if (is_int($value) || is_float($value)) {
            return (string) $value;
        }
        $problems[] = $key.': expected a scalar value';

        return null;
    }

    private static function currentBranch(string $repoDir): ?string
    {
        $cmd = 'git -C '.escapeshellarg($repoDir).' rev-parse --abbrev-ref HEAD 2>&1';
        $out = @shell_exec($cmd);
        $branch = is_string($out) ? trim($out) : '';
        if ($branch === '' || $branch === 'HEAD' || str_contains($branch, 'fatal')) {
            return null;
        }

        return $branch;
    }

    private static function sanitizeBranch(string $branch): string
    {
        return preg_replace('/[^A-Za-z0-9._-]/', '-', $branch) ?? 'unknown';
    }

    private static function checkType(array $spec, string $key, string $raw, array &$problems): void
    {
        $type = is_string($spec['type'] ?? null) ? $spec['type'] : 'string';
        $trimmed = trim($raw);
        switch ($type) {
            case 'integer':
                if (preg_match('/^-?\d+$/', $trimmed) !== 1) {
                    $problems[] = sprintf('%s: expected an integer, got "%s"', $key, $raw);
                }
                break;
            case 'number':
            case 'float':
                if (is_numeric($trimmed) === false) {
                    $problems[] = sprintf('%s: expected a number, got "%s"', $key, $raw);
                }
                break;
            case 'boolean':
            case 'bool':
                if (!in_array(strtolower($trimmed), ['true', 'false', '1', '0', 'yes', 'no', 'on', 'off'], true)) {
                    $problems[] = sprintf('%s: expected a boolean (true/false), got "%s"', $key, $raw);
                }
                break;
            default:
                break;
        }
    }

    private static function checkFormat(array $spec, string $key, string $raw, array &$problems): void
    {
        $format = $spec['format'] ?? null;
        if (!is_string($format)) {
            return;
        }
        switch ($format) {
            case 'uri':
            case 'url':
                $idx = strpos($raw, '://');
                if ($idx === false || $idx === 0 || preg_match('/\s/', $raw) === 1) {
                    $problems[] = sprintf("%s: does not satisfy format '%s': \"%s\"", $key, $format, $raw);
                    break;
                }
                $scheme = substr($raw, 0, $idx);
                if (!in_array($scheme, self::KNOWN_SCHEMES, true)) {
                    $best = self::bestScheme($scheme);
                    if ($best !== null) {
                        $problems[] = sprintf(
                            "%s: does not satisfy format '%s' — did you mean %s://%s?",
                            $key,
                            $format,
                            $best,
                            substr($raw, $idx + 3)
                        );
                    }
                }
                break;
            case 'email':
                $at = strpos($raw, '@');
                $ok = $at !== false && $at > 0 && strpos($raw, '.', $at + 1) > $at + 1
                    && preg_match('/\s/', $raw) !== 1;
                if (!$ok) {
                    $problems[] = sprintf("%s: does not satisfy format 'email': \"%s\"", $key, $raw);
                }
                break;
            case 'uuid':
                if (preg_match('/^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/', $raw) !== 1) {
                    $problems[] = sprintf("%s: does not satisfy format 'uuid': \"%s\"", $key, $raw);
                }
                break;
            default:
                break;
        }
    }

    private static function bestKeyMatch(string $key, array $candidates): ?string
    {
        $limit = max(1, min(3, intdiv(strlen($key), 3)));
        $best = null;
        $bestDistance = PHP_INT_MAX;
        foreach ($candidates as $candidate) {
            $distance = self::osaDistance($key, (string) $candidate);
            if ($distance <= $limit && $distance < $bestDistance) {
                $best = (string) $candidate;
                $bestDistance = $distance;
            }
        }

        return $best;
    }

    private static function bestScheme(string $scheme): ?string
    {
        $limit = max(1, min(3, intdiv(strlen($scheme), 3)));
        $best = null;
        $bestDistance = PHP_INT_MAX;
        foreach (self::KNOWN_SCHEMES as $candidate) {
            $distance = self::osaDistance($scheme, $candidate);
            if ($distance <= $limit && $distance < $bestDistance) {
                $best = $candidate;
                $bestDistance = $distance;
            }
        }

        return $best;
    }

    /** Optimal string alignment distance (catches ture→true style typos). */
    private static function osaDistance(string $a, string $b): int
    {
        $m = strlen($a);
        $n = strlen($b);
        $d = [];
        for ($i = 0; $i <= $m; $i++) {
            $d[$i][0] = $i;
        }
        for ($j = 0; $j <= $n; $j++) {
            $d[0][$j] = $j;
        }
        for ($i = 1; $i <= $m; $i++) {
            for ($j = 1; $j <= $n; $j++) {
                $cost = $a[$i - 1] === $b[$j - 1] ? 0 : 1;
                $d[$i][$j] = min($d[$i - 1][$j] + 1, $d[$i][$j - 1] + 1, $d[$i - 1][$j - 1] + $cost);
                if ($i > 1 && $j > 1 && $a[$i - 1] === $b[$j - 2] && $a[$i - 2] === $b[$j - 1]) {
                    $d[$i][$j] = min($d[$i][$j], $d[$i - 2][$j - 2] + 1);
                }
            }
        }

        return $d[$m][$n];
    }

    private static function mockValue(string $key): string
    {
        $hash = 0xcbf29ce484222325;
        for ($i = 0, $len = strlen($key); $i < $len; $i++) {
            $hash ^= ord($key[$i]);
            $hash = ($hash * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF;
        }

        return 'mock_'.sprintf('%016x%016x', $hash, $hash >> 1);
    }
}
