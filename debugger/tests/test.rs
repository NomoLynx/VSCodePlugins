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
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let asm_file_path = PathBuf::from(manifest_dir)
        .join("tests")
        .join("code.asm");
    
    let mut machine = Machine::default();

    let asm_prog_text = read_file_to_string(asm_file_path.to_str().unwrap());
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
