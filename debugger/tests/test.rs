use core_utils::filesystem::read_file_to_string;
use riscv_asm_lib::r5asm::assembler::*;
use std::path::PathBuf;

#[path = "../src/machine/mod.rs"]
mod machine;
#[path = "../src/memory/mod.rs"]
mod memory;
#[path = "../src/vm/mod.rs"]
mod vm;
#[path = "../src/trace/mod.rs"]
mod trace;
#[path = "../src/debug/mod.rs"]
mod debug;
#[path = "../src/debugger_error.rs"]
mod debugger_error;

use crate::machine::machine::Machine;

#[test]
fn test_machine_execution() {
    // Get the manifest directory and construct path to code.asm
       let asm_prog_text = r#"
.data
    a0: .word 0
    b0: .word 1

.text
main:
    add t0, t1, t2
    beq t3, t4, main
"#;
    
    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(&asm_prog_text).unwrap();
    machine.add_program(program);

    let t1 = machine.lookup_register("t1").unwrap();
    let t2 = machine.lookup_register("t2").unwrap();
    let t0 = machine.lookup_register("t0").unwrap();

    let default_hart_id = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(default_hart_id).unwrap();
    let pc = machine.get_default_hart().unwrap().pc;

    let hart = machine.get_default_hart_mut().unwrap();    
    hart.write_register(&t1, 10.into());
    hart.write_register(&t2, 20.into());
    

    let r = machine.step_hart(default_hart_id);
    let result = machine.get_default_hart().unwrap().read_register(&t0);
    
    assert!(r.is_ok());
    assert_eq!(result, 30.into());

    let r = machine.step_hart(default_hart_id);
    assert!(r.is_ok());

    let pc2 = machine.get_default_hart().unwrap().pc;
    assert_eq!(pc2, pc);
}

#[test]
fn test_data_load() {
    // Get the manifest directory and construct path to code.asm
    let asm_prog_text = r#"
.data
    a0: .word 0
    b0: .word 1

.text
main:
    la t0, b0
    lw t1, 0(t0)
"#;

    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(&asm_prog_text).unwrap();
    machine.add_program(program);

    let t1 = machine.lookup_register("t1").unwrap();
    let t2 = machine.lookup_register("t2").unwrap();
    let t0 = machine.lookup_register("t0").unwrap();

    let default_hart_id = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(default_hart_id).unwrap();
    let pc = machine.get_default_hart().unwrap().pc;

    let r = machine.step_hart(default_hart_id);
    assert!(r.is_ok());

    let r = machine.step_hart(default_hart_id);
    assert!(r.is_ok());

    let r = machine.step_hart(default_hart_id);
    assert!(r.is_ok());

    let has_inst = machine.has_inst_at_pc(default_hart_id);
    assert_eq!(has_inst, false);

    let result = machine.get_default_hart().unwrap().read_register(&t1);
    assert_eq!(result, 1.into());
}

#[test]
fn test_program_data_is_loaded_into_memory() {
    let asm_prog_text = r#"
.data
    a0: .word 0
    b0: .word 1

.text
main:
    la t0, b0
    lw t1, 0(t0)
"#;

    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(&asm_prog_text).unwrap();
    machine.add_program(program);

    assert_eq!(machine.memory.read_u32(0).unwrap(), 0);
    assert_eq!(machine.memory.read_u32(4).unwrap(), 1);
}

#[test]
fn test_jalr_uses_resolved_immediate() {
    let asm_prog_text = r#"
.text
main:
    addi t0, x0, 108
    jalr ra, 4(t0)
    addi t1, x0, 1
    addi t2, x0, 2
"#;

    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    machine.add_program(program);

    let t1 = machine.lookup_register("t1").unwrap();
    let t2 = machine.lookup_register("t2").unwrap();
    let ra = machine.lookup_register("ra").unwrap();

    let default_hart_id = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(default_hart_id).unwrap();

    machine.step_hart(default_hart_id).unwrap();
    machine.step_hart(default_hart_id).unwrap();
    machine.step_hart(default_hart_id).unwrap();

    let hart = machine.get_default_hart().unwrap();
    assert_eq!(hart.read_register(&t1), 0.into());
    assert_eq!(hart.read_register(&t2), 2.into());
    assert_eq!(hart.read_register(&ra), 108.into());
}
