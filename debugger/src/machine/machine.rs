use riscv_asm_lib::r5asm::asm_program::AsmProgram;

use crate::machine::hart::Hart;
use crate::machine::processor::Processor;
use crate::memory::memory::Memory;

pub type ProcessorId = usize;
pub type HartId = usize;
pub type ProgramId = usize;

pub struct Machine {
    pub processors: Vec<Processor>,

    pub programs: Vec<AsmProgram>,

    pub memory: Memory,
}