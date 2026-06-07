mod machine;
mod memory;
mod program;
mod vm;
mod trace;
mod debug;
mod debugger_error;

use riscv_asm_lib::r5asm::assembler::*;

use crate::machine::{hart::{
    CsrFile, FloatRegisterFile, Hart, IntegerRegisterFile, RegValue, VectorRegisterFile, VectorState
}, machine::Machine};

fn main() {
    let mut machine = Machine::default();

    let program = parse_asm_use_default_config(".text \r\n add t0, t1, t2").unwrap();
    machine.add_program(program);
    let hart = machine.get_hart_mut(0).unwrap();
    // t1 = 10
    hart.x.write(6, 10);

    // t2 = 20
    hart.x.write(7, 20);
    

    let r = machine.step_hart(0);
    let result = machine.get_hart(0).unwrap().x.read(5);
    
    println!("Result: {:?}", r);
    println!("t0: {}", result);
    
}
