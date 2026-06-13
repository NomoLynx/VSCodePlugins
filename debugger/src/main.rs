mod machine;
mod memory;
mod vm;
mod trace;
mod debug;
mod debugger_error;

use core_utils::filesystem::read_file_to_string;
use riscv_asm_lib::r5asm::assembler::*;

use crate::machine::machine::Machine;

fn main() {
    let mut machine = Machine::default();

    let asm_prog_text = read_file_to_string("code2.asm");
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

    let result = machine.get_default_hart().unwrap().read_register(&t1);
    assert_eq!(result, 1.into());
}
