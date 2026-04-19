## Plan: ASM instruction highlighting

✅ **Recommended approach:** use the source-location data already stored on parsed instructions and emit semantic tokens directly from the parsed AsmProgram.

I found that:

- the VS Code server already exposes semantic tokens
- the assembly parser already preserves source ranges for the instruction name and operands
- this means the LSP does **not** need to rediscover mnemonic spans by scanning raw text

---

### Steps

1. **Use the parser as the single source of truth**
   - Keep `asm_parse` as the only parse step in the LSP server.
   - Reuse the parsed `AsmProgram` already stored in the document state.

2. **Collect tokens from parsed instructions**
   - Walk the text-section items from `AsmProgram`.
   - For each instruction, use its stored name location from the parser instead of searching the source line manually.

3. **Convert parser locations into semantic tokens**
   - Translate the instruction source range into VS Code semantic token coordinates.
   - Emit the mnemonic as a `keyword` token.

4. **Keep phase 1 focused**
   - Highlight instruction mnemonics first.
   - Leave registers, labels, directives, and immediates for a follow-up phase, even though their locations are now available too.

5. **Add targeted regression tests**
   - Verify that a normal instruction produces one keyword token at the exact stored range.
   - Verify that a label plus instruction still highlights only the mnemonic span.
   - Verify that compressed instructions such as `c.addi` use the exact parser-provided location.
   - Verify that invalid input still reports diagnostics and does not break token generation.

---

### Relevant files

- VSCodePlugins/server/src/main.rs — semantic token provider and token generation
- VSCodePlugins/server/src/tests.rs — regression coverage for token spans
- riscv_asm_lib/src/r5asm/instruction.rs — source-range accessors such as the instruction-name location
- riscv_asm_lib/src/r5asm/asm_program.rs — parsed instruction container used by the LSP

---

### Verification

1. Build the Rust server and extension
2. Open a sample assembly file
3. Confirm mnemonics are highlighted at the exact parser-provided spans
4. Confirm diagnostics still work for invalid assembly

This revised plan is ready for further refinement.