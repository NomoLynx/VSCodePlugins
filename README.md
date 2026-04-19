# RiscV Language Support

A Visual Studio Code extension for RISC-V assembly development.

This extension targets files with the .rv.s extension and uses a Rust-based language server to provide parsing, diagnostics, and semantic highlighting.

## Current Features

- RISC-V assembly language support for .rv.s files
- parser-backed diagnostics for invalid assembly
- semantic highlighting for:
  - instruction mnemonics
  - register names
- separate token types for instructions and registers so their colors can be customized independently
- basic language configuration for comments and editor behavior

## Current Highlighting

The extension currently highlights:

- instructions as their own semantic token type
- registers as a separate semantic token type

The register color can be adjusted later through semantic token color customization rules.

## Project Structure

- src/extension.ts — VS Code client that starts the language server
- server/src/main.rs — Rust LSP server implementation
- language-configuration.json — editor configuration for the RISC-V language

## Prerequisites

- Node.js 18+
- Rust stable toolchain
- VS Code
- vsce for packaging

Install the packaging tool if needed:

```powershell
npm install -g @vscode/vsce
```

## Build From Source

```powershell
npm install
cargo build --release --manifest-path ./server/Cargo.toml
npm run compile
```

## Run in the Extension Development Host

1. Open this folder in VS Code.
2. Press F5.
3. In the new window, open or create a .rv.s file.
4. Type RISC-V assembly such as:

```asm
.text
start:
  addi x1, x2, 123
  c.addi x1, 1
```

You should see:

- instruction mnemonics highlighted
- register names highlighted with their own styling
- parse errors reported as diagnostics when the assembly is invalid

## Package as VSIX

```powershell
vsce package --allow-missing-repository
```

This generates a VSIX package in the project folder.

## Notes

- The extension currently uses the parser's source locations to place semantic tokens precisely.
- The server binary used by the extension is the release build under the server target folder.
- Colors can be tuned later without changing the parser logic.
