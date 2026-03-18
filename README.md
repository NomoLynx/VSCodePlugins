# Rust Keyword LSP Sample

This sample project demonstrates a VS Code extension that launches a Rust-based LSP server.

The language id is `rkw` and files ending in `.rkw` are handled by the extension. The server returns semantic tokens for common Rust keywords, so they are highlighted by your theme.

## Project Structure

- `src/extension.ts`: VS Code client that starts the server process.
- `server/src/main.rs`: Rust LSP server implementation.
- `language-configuration.json`: Basic editor behavior for the demo language.

## Prerequisites

- Node.js 18+
- Rust toolchain (stable)
- VS Code

## Build

```powershell
npm install
npm run build-server
npm run compile
```

## Run in VS Code

1. Open this folder (`RustKeywordLspSample`) in VS Code.
2. Press `F5` to launch the Extension Development Host.
3. In the new VS Code window, create `demo.rkw`.
4. Type text like:

```txt
fn main() {
  let mut count = 0
  if count == 0 {
    return
  }
}
```

Keywords such as `fn`, `let`, `mut`, `if`, and `return` should be highlighted.

## Notes

- This is a minimal sample focused on semantic tokens.
- If the server binary is missing, run `npm run build-server` again.
