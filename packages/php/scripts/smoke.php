<?php

declare(strict_types=1);

use Envy\Envy;
use Envy\EnvyException;

require __DIR__.'/../vendor/autoload.php';

$dir = sys_get_temp_dir().'/envy-php-smoke-'.getmypid();
@mkdir($dir.'/nested', 0777, true);

file_put_contents($dir.'/envy.yaml', <<<'YAML'
version: "1"
service: php-svc
config:
  PORT:
    type: integer
    default: 8080
  DATABASE_URL:
    type: string
    format: uri
    required: true
  MOCKED_KEY:
    type: string
    mock: true
YAML);

file_put_contents($dir.'/envy.local.yaml', <<<'YAML'
values:
  DATABASE_URL: "postgersql://u:p@localhost/db"
  DATABSE_URL: "typo"
YAML);

try {
    Envy::loadFrom($dir.'/nested');
    fwrite(STDERR, "FAIL: expected EnvyException\n");
    exit(1);
} catch (EnvyException $e) {
    $joined = implode("\n", $e->problems);
    if (!str_contains($joined, 'did you mean postgresql://')) {
        fwrite(STDERR, "FAIL: missing scheme suggestion:\n$joined\n");
        exit(1);
    }
    if (!str_contains($joined, 'did you mean DATABASE_URL?')) {
        fwrite(STDERR, "FAIL: missing key suggestion:\n$joined\n");
        exit(1);
    }
}

file_put_contents($dir.'/envy.local.yaml', <<<'YAML'
values:
  DATABASE_URL: "postgresql://u:p@localhost/db"
YAML);

$config = Envy::loadFrom($dir.'/nested');

if (($config['PORT'] ?? null) !== '8080') {
    fwrite(STDERR, "FAIL: default not applied\n");
    exit(1);
}
if (($config['DATABASE_URL'] ?? null) !== 'postgresql://u:p@localhost/db') {
    fwrite(STDERR, "FAIL: local value not applied\n");
    exit(1);
}
if (!str_starts_with((string) ($config['MOCKED_KEY'] ?? ''), 'mock_')) {
    fwrite(STDERR, "FAIL: mock not generated\n");
    exit(1);
}

echo "php smoke: OK (upward search, defaults, mocks, suggestions)\n";
self_cleanup($dir);

function self_cleanup(string $dir): void
{
    foreach (['envy.yaml', 'envy.local.yaml'] as $file) {
        @unlink($dir.'/'.$file);
    }
    @rmdir($dir.'/nested');
    @rmdir($dir);
}
