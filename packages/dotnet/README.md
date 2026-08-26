# Envy.Config (.NET)

Load and validate `envy.yaml` natively in .NET — ASP.NET Core, Worker Services,
console apps. No CLI required.

## Install

```bash
dotnet add package Envy.Config
```

(Once published to NuGet; until then reference this csproj.)

## Usage

```csharp
using Envy;

var config = EnvyLoader.Load();          // searches upward from CWD

string dbUrl = config["DATABASE_URL"];
int port     = int.Parse(config["PORT"]);

// typed accessor:
string? secret = EnvyLoader.Get("API_SECRET");
```

### ASP.NET Core wiring

```csharp
builder.Configuration.AddInMemoryCollection(
    EnvyLoader.Load().ToDictionary(kvp => kvp.Key, kvp => (string?)kvp.Value));
```

## Behaviour (identical to the Rust core)

- upward search for `envy.yaml`
- precedence: environment variables → branch overlay (`envy.local.<branch>.yaml`) → `envy.local.yaml` → schema default → generated mock
- validates integer / number / boolean types and `uri` / `email` / `uuid` formats
- typo'd keys and schemes reported with "did you mean …?" suggestions
- throws one `EnvyLoader.EnvyException` listing every problem at once
- cached per schema location (`Load()` is cheap after first call)

Targets `netstandard2.0` + `net8.0`.

## Building

```bash
dotnet build -c Release
```
