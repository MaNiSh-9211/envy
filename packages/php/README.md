# envy-php (Composer)

Load and validate `envy.yaml` natively in PHP — Laravel, Symfony, WordPress,
plain scripts. No CLI required.

## Install

```bash
composer require manish-9211/envy-php
```

## Usage

```php
use Envy\Envy;
use Envy\EnvyException;

try {
    $config = Envy::load();               // searches upward from getcwd()

    $dbUrl = $config['DATABASE_URL'];
    $port  = (int) ($config['PORT'] ?? 8080);
} catch (EnvyException $e) {
    foreach ($e->problems as $problem) {   // every problem in one pass
        error_log($problem);
    }
}
```

## Behaviour (identical to the Rust core)

- upward search for `envy.yaml`
- precedence: `getenv()` → branch overlay (`envy.local.<branch>.yaml`) → `envy.local.yaml` → schema default → generated mock
- validates integer / number / boolean types and `uri` / `email` / `uuid` formats
- typo'd keys and schemes reported with "did you mean …?" suggestions
- results cached per schema location

## Development

```bash
composer install
composer smoke     # end-to-end fixture test without PHPUnit
```
