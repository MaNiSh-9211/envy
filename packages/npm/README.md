# envy-cli (npm)

Thin installer that downloads the native `envy` Rust binary for your platform
from [GitHub Releases](https://github.com/MaNiSh-9211/envy/releases).

```bash
npm install -g envy-cli
envy run npm run dev
```

Pin a specific version:

```bash
ENVY_VERSION=v0.1.0 npm install -g envy-cli
```

The package ships zero JavaScript logic beyond download + exec.
