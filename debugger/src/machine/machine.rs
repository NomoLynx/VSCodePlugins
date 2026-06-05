use riscv_asm_lib::r5asm::asm_program::AsmProgram;
use riscv_asm_lib::r5asm::instruction::Instruction;

use crate::machine::hart::Hart;
use crate::machine::processor::Processor;
use crate::memory::memory::Memory;

pub type ProcessorId = usize;
pub type HartId = u64;
pub type ProgramId = usize;

pub struct Machine {
    pub processors: Vec<Processor>,

    pub programs: Vec<AsmProgram>,

    pub memory: Memory,
}

impl Machine {

    pub fn add_program(
        &mut self,
        program: AsmProgram,
    ) -> ProgramId {

        let id = self.programs.len();

        self.programs.push(program);

        id
    }

    pub fn add_processor(
        &mut self,
        processor: Processor,
    ) {

        self.processors.push(processor);
    }

    pub fn get_hart_mut(
        &mut self,
        hart_id: HartId,
    ) -> Option<&mut Hart> {

        for processor in &mut self.processors {

            for hart in &mut processor.harts {

                if hart.id == hart_id {

                    return Some(hart);
                }
            }
        }

        None
    }

    fn fetch_inst(
        &self,
        hart: &Hart,
    ) -> Option<&Instruction> {

        let program = &self.programs[hart.program_id];
        let text_items = program.get_text_section_items();
        let item = text_items.get(hart.pc)
                                            .and_then(|x| x.get_inc());
        item
    }

    pub fn step_hart(&mut self, hart_id: HartId,) {

        let (program_id, pc) = {

            let hart =
                self.get_hart_mut(hart_id)
                    .expect("invalid hart");

            (hart.program_id, hart.pc)
        };

        if let Some(inst) = 
            self.programs[program_id]
                .get_text_section_items()
                .get(pc)
                .and_then(|x| x.get_inc())
                .cloned() {

            self.execute_inst(
                hart_id,
                &inst,
            );
        }

    }

    fn execute_inst(
        &mut self,
        hart_id: u64,
        inst: &Instruction,
    ) {
        let hart = self.get_hart_mut(hart_id as u64).unwrap();
        
    }
}