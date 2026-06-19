mod machine;
mod memory;
mod vm;
mod trace;
mod debug;
mod debugger_error;

use core_utils::filesystem::*;
use core_utils::debug::*;
use riscv_asm_lib::r5asm::assembler::*;

use crate::machine::machine::Machine;

fn main() {
    eprint!("Hello, Debugger!");

    std::thread::sleep(std::time::Duration::from_secs(30));
}
