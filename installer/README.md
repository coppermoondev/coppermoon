# CopperMoon Installer

Cross-platform install scripts for [CopperMoon](https://github.com/coppermoondev/coppermoon).

Installs **coppermoon**, **harbor**, and **shipyard** binaries.

## Quick Install

### Linux / macOS

```sh
curl -fsSL https://coppermoon.dev/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://coppermoon.dev/install.ps1 | iex
```

## Options

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `COPPERMOON_INSTALL_DIR` | `~/.coppermoon/bin` (Unix) / `%USERPROFILE%\.coppermoon\bin` (Win) | Custom install directory |
| `COPPERMOON_VERSION` | `latest` | Install a specific version (e.g. `0.1.0`) |
| `COPPERMOON_NO_MODIFY_PATH` | `0` | Set to `1` to skip PATH modification |

### Examples

```sh
# Install specific version
COPPERMOON_VERSION=0.2.0 curl -fsSL https://coppermoon.dev/install.sh | sh

# Custom install directory
COPPERMOON_INSTALL_DIR=/usr/local/bin curl -fsSL https://coppermoon.dev/install.sh | sh

# Skip PATH modification
COPPERMOON_NO_MODIFY_PATH=1 curl -fsSL https://coppermoon.dev/install.sh | sh
```

```powershell
# Install specific version (Windows)
$env:COPPERMOON_VERSION="0.2.0"; irm https://coppermoon.dev/install.ps1 | iex
```

## Manual Install

Download the archive for your platform from [GitHub Releases](https://github.com/coppermoondev/coppermoon/releases), extract, and add to your PATH.

## Uninstall

### Linux / macOS

```sh
rm -rf ~/.coppermoon
# Then remove the CopperMoon lines from your shell profile (~/.zshrc, ~/.bashrc, etc.)
```

### Windows

```powershell
Remove-Item -Recurse -Force "$env:USERPROFILE\.coppermoon"
# Remove from PATH via System Settings > Environment Variables
```
