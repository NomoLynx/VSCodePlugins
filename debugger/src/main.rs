mod machine;
mod memory;
mod program;
mod vm;
mod trace;
mod debug;
mod debugger_error;

use riscv_asm_lib::r5asm::assembler::*;

use crate::machine::machine::Machine;

fn main() {
    let mut machine = Machine::default();

    let program = parse_asm_use_default_config(".text \r\n add t0, t1, t2").unwrap();
    machine.add_program(program);
    let t1 = machine.registers.get_register_value(Some(&"t1".to_string())).unwrap();
    let t2 = machine.registers.get_register_value(Some(&"t2".to_string())).unwrap();

    let hart = machine.get_default_hart_mut().unwrap();
    // t1 = 10
    hart.x.write(t1 as usize, 10);

    // t2 = 20
    hart.x.write(t2 as usize, 20);
    

    let r = machine.step_hart(0);
    let result = machine.get_default_hart().unwrap().x.read(5);
    
    println!("Result: {:?}", r);
    println!("t0: {}", result);
    
}
