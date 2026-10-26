# Den Editor

<div align="center" style="text-align: center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/den-icon-dark.svg">
  <img alt="Den icon" src="docs/assets/den-icon.svg" width="83" height="100">
</picture>
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/den-logo-dark.svg">
  <img alt="Den logo" src="docs/assets/den-logo.svg" width="200" height="100">
</picture>

<br>

<a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue" style="display: inline-block"></a>
&nbsp;
<a href="https://github.com/K16858/den-editor/releases"><img alt="Release" src="https://img.shields.io/github/v/release/K16858/den-editor" style="display: inline-block"></a>

</div>

Den is a text editor for the terminal, written in Rust.
Editing, a file tree, a shell, and a debugger share one screen.

## Features

- Search, replace, undo, and save
- Create files and folders from the sidebar
- Syntax highlighting driven by TOML. Rust, Python, Go, JavaScript, C, and Markdown are included
- Integrated terminal
- Debugging for Python (debugpy), Rust (CodeLLDB), and Go (Delve). Other adapters can be added in TOML

## Install

The installers download the latest release and check its checksum.

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/K16858/den-editor/main/install.sh | bash

# Windows (PowerShell)
irm https://raw.githubusercontent.com/K16858/den-editor/main/install.ps1 | iex
```

Or build it yourself:

```bash
cargo build --release
```

## Usage

```text
den [path]
den --version
```

With no path, Den opens the current directory. A directory becomes the workspace. A file is opened directly. `--version` and `-V` print the version and exit.

## Configuration

Den starts with built-in defaults. Put overrides here:

- Linux / macOS: `~/.config/den/`
- Windows: `%APPDATA%\den\`

| File                    | Purpose                          |
| ----------------------- | -------------------------------- |
| `colors.toml`           | Theme colors                     |
| `languages/<name>.toml` | Highlighting and file extensions |
| `debuggers/<name>.toml` | A debug adapter                  |

Samples are in `docs/examples/default/`.

## Keys

| Action                  | Key                   |
| ----------------------- | --------------------- |
| Quit (asks if unsaved)  | Ctrl+Q                |
| Save                    | Ctrl+S                |
| Search / Replace        | Ctrl+F / Ctrl+H       |
| Toggle sidebar          | Ctrl+B                |
| Focus editor / terminal | Ctrl+1 / Ctrl+2       |
| Toggle terminal         | Ctrl+@                |
| New file / folder       | Ctrl+N / Ctrl+Shift+N |
| Cancel                  | Esc                   |
| Start / stop debug      | F5 / Shift+F5         |
| Breakpoint              | F9                    |
| Step over / in / out    | F10 / F11 / Shift+F11 |
| Pause / restart         | F6 / Shift+F6         |
| Continue                | Ctrl+R                |

## License

[MIT](LICENSE)
