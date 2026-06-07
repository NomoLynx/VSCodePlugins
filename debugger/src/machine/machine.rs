use riscv_asm_lib::r5asm::asm_program::AsmProgram;
use riscv_asm_lib::r5asm::instruction::Instruction;
use riscv_asm_lib::r5asm::opcode::OpCode;
use riscv_asm_lib::r5asm::register::Register;

use crate::debugger_error::DebuggerError;
use crate::machine::hart::Hart;
use crate::machine::processor::Processor;
use crate::machine::register_ref::{RegisterRef, RegisterType};
use crate::memory::memory::Memory;

pub type ProcessorId = usize;
pub type HartId = u64;
pub type ProgramId = usize;

pub struct Machine {
    pub processors: Vec<Processor>,

    pub programs: Vec<AsmProgram>,

    pub memory: Memory,

    pub registers: Register,
}

impl Machine {

    pub fn new() -> Self {
        let mut r = Self {
            processors: vec![],
            programs: vec![],
            memory: Memory::default(),

            registers: Register::new(),
        };

        r.add_processor(Processor::default());
        r
    }

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

    pub fn get_hart(&self, hart_id: HartId) -> Option<&Hart> {
        for processor in &self.processors {

            for hart in &processor.harts {

                if hart.id == hart_id {

                    return Some(hart);
                }
            }
        }

        None
    }

    pub fn get_hart_mut(&mut self, hart_id: HartId) -> Option<&mut Hart> {
        for processor in &mut self.processors {

            for hart in &mut processor.harts {

                if hart.id == hart_id {

                    return Some(hart);
                }
            }
        }

        None
    }

    pub fn get_default_hart(&self) -> Option<&Hart> {
        self.get_hart(0)
    }

    pub fn get_default_hart_mut(&mut self) -> Option<&mut Hart> {
        self.get_hart_mut(0)
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

    pub fn step_hart(&mut self, hart_id: HartId) -> Result<(), DebuggerError> {

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
            )
        }
        else {
            Ok(())
        }
    }

    fn xreg(
        &self,
        name: &Option<String>,
    ) -> usize {

        self.registers
            .get_register_value(name.as_ref())
            .unwrap() as usize
    }

    fn get_x(
        &self,
        hart_id: HartId,
        reg: Option<&String>,
    ) -> u64 {

        let idx =
            self.registers
                .get_register_value(reg)
                .unwrap() as usize;

        let hart = self.get_hart(hart_id).unwrap();
        hart.x.regs[idx].value
    }

    fn set_x(
        &mut self,
        hart_id: HartId,
        reg: Option<&String>,
        value: u64,
    ) {

        let idx =
            self.registers
                .get_register_value(reg)
                .unwrap() as usize;

        let hart = self.get_hart_mut(hart_id).unwrap();
        if idx != 0 {
            hart.x.regs[idx].value = value;
        }
    }

    fn execute_inst(&mut self, hart_id: HartId, inst: &Instruction) -> Result<(), DebuggerError> {
        let opcode = inst.get_op_code()
                    .map_err(|x| DebuggerError::GeneralError(x.get_error_message()))?;

        match opcode {
            OpCode::Add => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs.wrapping_add(rhs),
                );

                self.next_pc(hart_id)?;
            }
            _ => {
                return Err(DebuggerError::GeneralError(format!("unsupported instruction: {:?}", opcode)));
            }
        }

        Ok(())
    }

    fn next_pc(&mut self, hart_id: HartId) -> Result<(), DebuggerError> {
        let err_msg = format!("invalid hart: {}", hart_id);
        let hart = self.get_hart_mut(hart_id).ok_or_else(|| DebuggerError::GeneralError(err_msg))?;
        hart.next_pc();
        Ok(())
    }

    pub fn lookup_register(&self, name: &str) -> Option<RegisterRef> {
        let index = self.registers.get_register_value(Some(&name.to_string()));
        match index {
            Ok(idx) => {
                let mut r = RegisterRef {
                    reg_type: RegisterType::Integer,
                    index: idx as usize,
                };

                let reg_type = if Register::is_float_register_name(name) {
                    RegisterType::Float
                }
                else if Register::is_vector_register_name(name) {
                    RegisterType::Vector
                }
                else if Register::is_csr_register_name(name) {
                    RegisterType::Csr
                }
                else {
                    RegisterType::Integer
                };

                r.reg_type = reg_type;

                Some(r)
            }
            Err(_) => None,
        }
    }
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}