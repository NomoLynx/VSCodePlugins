use riscv_asm_lib::r5asm::{asm_error::AsmError, asm_program::AsmProgram, assembler::parse_asm, code_gen_config::CodeGenConfiguration};

pub (crate) fn asm_parse(input:&str) -> Result<AsmProgram, AsmError> {
    let mut config = CodeGenConfiguration::default();
    config.set_replace_pseudo_code(false);
    let asm_prog = parse_asm(input, &mut config);
    asm_prog
}