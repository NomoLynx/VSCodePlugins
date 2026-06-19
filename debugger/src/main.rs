mod machine;
mod memory;
mod vm;
mod trace;
mod debug;
mod debugger_error;

use core_utils::filesystem::*;
use core_utils::debug::*;
use core_utils::log::init_logger;
use riscv_asm_lib::r5asm::assembler::*;

use crate::machine::machine::Machine;

use log::{info, error};

fn main() {
    log4rs::init_file("log4rs.yaml", Default::default()).unwrap();

    info!("Hello, Debugger!!!!!!");
    error!("Hello, Debugger error!!!!!!");  
    eprintln!("Hello, Debugger!!!!!!");

    std::thread::sleep(std::time::Duration::from_secs(30));
}
