
use riscv_asm_lib::r5asm::asm_program::AsmProgram;
use riscv_asm_lib::r5asm::imm::Imm::{self};
use riscv_asm_lib::r5asm::instruction::Instruction;
use riscv_asm_lib::r5asm::opcode::OpCode;
use riscv_asm_lib::r5asm::register::Register;

use crate::debugger_error::DebuggerError;
use crate::machine::hart::{Hart, PC_INCREMENT};
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

    pub fn add_program(&mut self, program: AsmProgram) -> ProgramId {
        let id = self.programs.len();
        self.load_program_memory(&program);
        self.programs.push(program);

        id
    }

    fn load_program_memory(&mut self, program: &AsmProgram) {
        for item in program.get_non_text_section_items() {
            let Some(directive) = item.get_directive() else {
                continue;
            };

            let Some(machine_code) = directive.get_machine_code() else {
                continue;
            };

            let bytes = machine_code.get_code_data().to_vec();
            if bytes.is_empty() {
                continue;
            }

            self.memory.write_bytes(item.get_offset() as u64, &bytes);
        }
    }

    pub fn add_processor(&mut self, processor: Processor) {
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

    pub fn get_processor_from_hart_id(&self, hart_id: HartId) -> Option<&Processor> {
        for processor in &self.processors {
            for hart in &processor.harts {
                if hart.id == hart_id {
                    return Some(processor);
                }
            }
        }

        None
    }

    pub fn get_default_hart_id(&self) -> HartId {
        0
    }

    pub fn get_default_hart(&self) -> Option<&Hart> {
        self.get_hart(self.get_default_hart_id())
    }

    pub fn get_default_hart_mut(&mut self) -> Option<&mut Hart> {
        let default_hart_id = self.get_default_hart_id();
        self.get_hart_mut(default_hart_id)
    }

    pub fn init_hart_to_entry_point(&mut self, hart_id: HartId) -> Result<(), DebuggerError> {
        let err_msg = format!("invalid hart: {}", hart_id);
        let program_id = {
            let hart = self.get_hart_mut(hart_id)
                                    .ok_or_else(|| DebuggerError::GeneralError(err_msg))?;
            hart.program_id
        };
        let entry_point = self.programs[program_id]
                                                    .get_entry_address2()
                                                    .map_err(|x| DebuggerError::GeneralError(x.get_error_message()))?;
        let hart = self.get_hart_mut(hart_id).unwrap();
        hart.pc = entry_point;
        Ok(())
    }

    /// fetch instruction for given hart, return None if no instruction found (e.g. pc is out of range)
    fn fetch_inst(&self, hart: &Hart) -> Option<&Instruction> {
        let program = &self.programs[hart.program_id];
        let item = program
            .get_text_section_items()
            .into_iter()
            .find(|x| x.get_offset() == hart.pc)
            .and_then(|x| x.get_inc());
        item
    }

    /// get instruction offset for given hart, return None if no instruction found (e.g. pc is out of range)
    fn get_inst_offset(&self, hart: &Hart) -> Option<usize> {
        let program = &self.programs[hart.program_id];
        let item = program
            .get_text_section_items()
            .into_iter()
            .find(|x| x.get_offset() == hart.pc);
        item.map(|x| x.get_offset())
    }

    /// get current instruction target address for given hart, 
    /// return None if no instruction found (e.g. pc is out of range)
    fn get_inst_target(&self, hart_id: HartId) -> Option<usize> {
        let hart = self.get_hart(hart_id)?;
        let inst = self.fetch_inst(hart)?;

        Some(inst.get_virtual_address() as usize)
    }

    /// check if there is an instruction at current pc of given hart
    pub fn has_inst_at_pc(&self, hart_id: HartId) -> bool {
        let hart = self.get_hart(hart_id);
        if let Some(hart) = hart {
            self.fetch_inst(hart).is_some()
        } else {
            false
        }
    }

    pub fn step_hart(&mut self, hart_id: HartId) -> Result<(), DebuggerError> {

        let (program_id, pc) = {
            let hart = self.get_hart_mut(hart_id)
                                    .expect("invalid hart");

            (hart.program_id, hart.pc)
        };

        let prog = &self.programs[program_id];
        if let Some(inst) = 
            prog.get_text_section_items()
                .into_iter()
                .find(|x| x.get_offset() == pc && x.is_inc())
                .and_then(|x| x.get_inc())
                .cloned() {
            self.execute_inst(hart_id, &inst)
        }
        else {
            if prog.get_text_section_items().is_empty() {
                return Err(DebuggerError::GeneralError(format!("no instructions found in program id: {}", program_id)));
            }

            Err(DebuggerError::GeneralError(format!("no instruction found at pc: {}", pc)))
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

    fn get_f32(&self, hart_id: HartId, reg: Option<&String>) -> f32 {
        let idx =
            self.registers
                .get_register_value(reg)
                .unwrap() as usize;

        let hart = self.get_hart(hart_id).unwrap();
        hart.f.regs[idx].value as f32
    }

    fn set_f32(&mut self, hart_id: HartId, reg: Option<&String>, value: f32) {
        let idx =
            self.registers
                .get_register_value(reg)
                .unwrap() as usize;

        let hart = self.get_hart_mut(hart_id).unwrap();
        hart.f.regs[idx].value = value as f64;
    }

    fn get_f(&self, hart_id: HartId, reg: Option<&String>) -> f64 {
        let idx =
            self.registers
                .get_register_value(reg)
                .unwrap() as usize;

        let hart = self.get_hart(hart_id).unwrap();
        hart.f.regs[idx].value
    }

    pub fn get_f_bits(&self, hart_id: HartId, reg: Option<&String>) -> u64 {
        self.get_f(hart_id, reg)
            .to_bits()
    }

    pub fn set_f_bits(&mut self, hart_id: HartId, reg: Option<&String>, bits: u64) {
        self.set_f(hart_id, reg, f64::from_bits(bits));
    }

    fn set_f(&mut self, hart_id: HartId, reg: Option<&String>, value: f64) {
        let idx =
            self.registers
                .get_register_value(reg)
                .unwrap() as usize;

        let hart = self.get_hart_mut(hart_id).unwrap();
        hart.f.regs[idx].value = value;
    }

    fn get_x(&self, hart_id: HartId, reg: Option<&String>) -> u64 {
        let idx =
            self.registers
                .get_register_value(reg)
                .unwrap() as usize;

        let hart = self.get_hart(hart_id).unwrap();
        hart.x.regs[idx].value
    }

    fn set_x(&mut self, hart_id: HartId, reg: Option<&String>, value: u64) {
        let idx =
            self.registers
                .get_register_value(reg)
                .unwrap() as usize;

        let hart = self.get_hart_mut(hart_id).unwrap();
        if idx != 0 {
            hart.x.regs[idx].value = value;
        }
    }

    fn get_resolved_u64(&self, hart_id: HartId, inst: &Instruction) -> u64 {
        if inst.get_rel_fun().is_some() {
            inst.get_virtual_address() as u64
        } else {
            self.get_u64_from_imm(hart_id, inst.get_imm())
        }
    }

    fn get_resolved_i64(&self, hart_id: HartId, inst: &Instruction) -> i64 {
        if inst.get_rel_fun().is_some() {
            inst.get_virtual_address() as i32 as i64
        } else {
            self.get_i64_from_imm(hart_id, inst.get_imm())
        }
    }

    fn binary_operation(&mut self, hart_id: HartId, inst: &Instruction, op: impl Fn(u64, u64) -> u64) -> Result<(), DebuggerError> {
        let lhs = self.get_x(hart_id, inst.get_r1());
        let rhs = self.get_x(hart_id, inst.get_r2());
        self.set_x(hart_id, inst.get_r0(), op(lhs, rhs));
        self.next_pc(hart_id)
    }

    fn execute_inst(&mut self, hart_id: HartId, inst: &Instruction) -> Result<(), DebuggerError> {
        let opcode = inst.get_op_code()
                    .map_err(|x| DebuggerError::GeneralError(x.get_error_message()))?;

        match opcode {
            OpCode::Add => self.binary_operation(hart_id, &inst, u64::wrapping_add)?,
            OpCode::Sub => self.binary_operation(hart_id, &inst, u64::wrapping_sub)?,
            OpCode::And => self.binary_operation(hart_id, &inst, |a, b| a & b)?,
            OpCode::Or  => self.binary_operation(hart_id, &inst, |a, b| a | b)?,
            OpCode::Xor => self.binary_operation(hart_id, &inst, |a, b| a ^ b)?,
            OpCode::Sll => self.binary_operation(hart_id, &inst, |a, b| a << (b & 0x3f) as u32)?,
            OpCode::Srl => self.binary_operation(hart_id, &inst, |a, b| a >> (b & 0x3f) as u32)?,
            OpCode::Sra => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i64;
                let rhs = self.get_x(hart_id, inst.get_r2());

                let shamt = (rhs & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    (lhs >> shamt) as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Andn => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), lhs & !rhs);
                self.next_pc(hart_id)?;
            }
            OpCode::Orn => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), lhs | !rhs);
                self.next_pc(hart_id)?;
            }
            OpCode::Xnor => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), !(lhs ^ rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Rol => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                let shamt = (rhs & 0x3f) as u32;
                self.set_x(hart_id, inst.get_r0(), lhs.rotate_left(shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Ror => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                let shamt = (rhs & 0x3f) as u32;
                self.set_x(hart_id, inst.get_r0(), lhs.rotate_right(shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Rori => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = (self.get_resolved_u64(hart_id, inst) & 0x3f) as u32;
                self.set_x(hart_id, inst.get_r0(), lhs.rotate_right(shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Clz => {
                let val = self.get_x(hart_id, inst.get_r1());
                self.set_x(hart_id, inst.get_r0(), val.leading_zeros() as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Ctz => {
                let val = self.get_x(hart_id, inst.get_r1());
                self.set_x(hart_id, inst.get_r0(), val.trailing_zeros() as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Cpop => {
                let val = self.get_x(hart_id, inst.get_r1());
                self.set_x(hart_id, inst.get_r0(), val.count_ones() as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Sextb => {
                let val = self.get_x(hart_id, inst.get_r1()) as i8 as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Sexth => {
                let val = self.get_x(hart_id, inst.get_r1()) as i16 as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Zexth => {
                let val = self.get_x(hart_id, inst.get_r1()) as u16 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Orcb => {
                let val = self.get_x(hart_id, inst.get_r1());
                let mut result: u64 = 0;
                for i in 0..8 {
                    if (val >> (i * 8)) & 0xff != 0 {
                        result |= 0xffu64 << (i * 8);
                    }
                }
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Rev8 => {
                let val = self.get_x(hart_id, inst.get_r1());
                let result = val.swap_bytes();
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Bclr => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), lhs & !(1u64 << (rhs & 0x3f)));
                self.next_pc(hart_id)?;
            }
            OpCode::Bclri => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = self.get_resolved_u64(hart_id, inst) & 0x3f;
                self.set_x(hart_id, inst.get_r0(), lhs & !(1u64 << shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Bset => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), lhs | (1u64 << (rhs & 0x3f)));
                self.next_pc(hart_id)?;
            }
            OpCode::Bseti => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = self.get_resolved_u64(hart_id, inst) & 0x3f;
                self.set_x(hart_id, inst.get_r0(), lhs | (1u64 << shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Bext => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), (lhs >> (rhs & 0x3f)) & 1);
                self.next_pc(hart_id)?;
            }
            OpCode::Bexti => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = self.get_resolved_u64(hart_id, inst) & 0x3f;
                self.set_x(hart_id, inst.get_r0(), (lhs >> shamt) & 1);
                self.next_pc(hart_id)?;
            }
            OpCode::Binv => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), lhs ^ (1u64 << (rhs & 0x3f)));
                self.next_pc(hart_id)?;
            }
            OpCode::Binvi => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = self.get_resolved_u64(hart_id, inst) & 0x3f;
                self.set_x(hart_id, inst.get_r0(), lhs ^ (1u64 << shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Min => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i64;
                let rhs = self.get_x(hart_id, inst.get_r2()) as i64;
                self.set_x(hart_id, inst.get_r0(), lhs.min(rhs) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Minu => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), lhs.min(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Max => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i64;
                let rhs = self.get_x(hart_id, inst.get_r2()) as i64;
                self.set_x(hart_id, inst.get_r0(), lhs.max(rhs) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Maxu => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), lhs.max(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Clzw => {
                let val = self.get_x(hart_id, inst.get_r1()) as u32;
                self.set_x(hart_id, inst.get_r0(), (val.leading_zeros() as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Ctzw => {
                let val = self.get_x(hart_id, inst.get_r1()) as u32;
                self.set_x(hart_id, inst.get_r0(), (val.trailing_zeros() as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Cpopw => {
                let val = self.get_x(hart_id, inst.get_r1()) as u32;
                self.set_x(hart_id, inst.get_r0(), (val.count_ones() as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Rolw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as u32;
                let rhs = self.get_x(hart_id, inst.get_r2());
                let shamt = (rhs & 0x1f) as u32;
                self.set_x(hart_id, inst.get_r0(), (lhs.rotate_left(shamt) as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Rorw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as u32;
                let rhs = self.get_x(hart_id, inst.get_r2());
                let shamt = (rhs & 0x1f) as u32;
                self.set_x(hart_id, inst.get_r0(), (lhs.rotate_right(shamt) as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Roriw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as u32;
                let shamt = (self.get_resolved_u64(hart_id, inst) & 0x1f) as u32;
                self.set_x(hart_id, inst.get_r0(), (lhs.rotate_right(shamt) as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Slt => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i64;
                let rhs = self.get_x(hart_id, inst.get_r2()) as i64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < rhs { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Sltu => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < rhs { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Mul => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs.wrapping_mul(rhs),
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Mulh => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i64 as i128;
                let rhs = self.get_x(hart_id, inst.get_r2()) as i64 as i128;
                let result = (lhs.wrapping_mul(rhs) >> 64) as u64;

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Mulhsu => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i64 as i128;
                let rhs = self.get_x(hart_id, inst.get_r2()) as i128;
                let result = (lhs.wrapping_mul(rhs) >> 64) as u64;

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Mulhu => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as u128;
                let rhs = self.get_x(hart_id, inst.get_r2()) as u128;
                let result = (lhs.wrapping_mul(rhs) >> 64) as u64;

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Div => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i64;
                let rhs = self.get_x(hart_id, inst.get_r2()) as i64;

                let result = if rhs == 0 {
                    u64::MAX
                } else if lhs == i64::MIN && rhs == -1 {
                    lhs as u64
                } else {
                    lhs.wrapping_div(rhs) as u64
                };

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Divu => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                let result = if rhs == 0 {
                    u64::MAX
                } else {
                    lhs.wrapping_div(rhs)
                };

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Rem => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i64;
                let rhs = self.get_x(hart_id, inst.get_r2()) as i64;

                let result = if rhs == 0 {
                    lhs as u64
                } else if lhs == i64::MIN && rhs == -1 {
                    0u64
                } else {
                    lhs.wrapping_rem(rhs) as u64
                };

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Remu => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                let result = if rhs == 0 {
                    lhs
                } else {
                    lhs.wrapping_rem(rhs)
                };

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Mulw => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                let result = (lhs.wrapping_mul(rhs) as i32 as i64) as u64;

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Divw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i32;
                let rhs = self.get_x(hart_id, inst.get_r2()) as i32;

                let result = if rhs == 0 {
                    u64::MAX
                } else if lhs == i32::MIN && rhs == -1 {
                    lhs as u64
                } else {
                    lhs.wrapping_div(rhs) as i32 as i64 as u64
                };

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Divuw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as u32;
                let rhs = self.get_x(hart_id, inst.get_r2()) as u32;

                let result = if rhs == 0 {
                    u64::MAX
                } else {
                    (lhs.wrapping_div(rhs) as i32 as i64) as u64
                };

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Remw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i32;
                let rhs = self.get_x(hart_id, inst.get_r2()) as i32;

                let result = if rhs == 0 {
                    lhs as u64
                } else if lhs == i32::MIN && rhs == -1 {
                    0u64
                } else {
                    lhs.wrapping_rem(rhs) as i32 as i64 as u64
                };

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Remuw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as u32;
                let rhs = self.get_x(hart_id, inst.get_r2()) as u32;

                let result = if rhs == 0 {
                    lhs as u64
                } else {
                    (lhs.wrapping_rem(rhs) as i32 as i64) as u64
                };

                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Sh1add => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), (lhs << 1).wrapping_add(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Sh2add => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), (lhs << 2).wrapping_add(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Sh3add => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), (lhs << 3).wrapping_add(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Slliuw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as u32;
                let shamt = self.get_resolved_u64(hart_id, inst) & 0x3f;
                self.set_x(hart_id, inst.get_r0(), (lhs as u64) << shamt);
                self.next_pc(hart_id)?;
            }
            OpCode::Adduw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as u32 as u64;
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), lhs.wrapping_add(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Sh1adduw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as u32 as u64;
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), (lhs << 1).wrapping_add(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Sh2adduw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as u32 as u64;
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), (lhs << 2).wrapping_add(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Sh3adduw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as u32 as u64;
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), (lhs << 3).wrapping_add(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Clmul => {
                let mut result: u64 = 0;
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                for i in 0..64 {
                    if (rhs >> i) & 1 != 0 {
                        result ^= lhs << i;
                    }
                }
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Clmulr => {
                let mut result: u64 = 0;
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                for i in 0..64 {
                    if (rhs >> i) & 1 != 0 {
                        result ^= lhs >> (63 - i);
                    }
                }
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Clmulh => {
                let mut result: u64 = 0;
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                for i in 1..64 {
                    if (rhs >> i) & 1 != 0 {
                        result ^= lhs >> (64 - i);
                    }
                }
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Czeroeqz => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), if rhs == 0 { 0 } else { lhs });
                self.next_pc(hart_id)?;
            }
            OpCode::Czeronez => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());
                self.set_x(hart_id, inst.get_r0(), if rhs != 0 { 0 } else { lhs });
                self.next_pc(hart_id)?;
            }
            OpCode::Addw => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    (lhs.wrapping_add(rhs) as i32 as i64) as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Subw => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    (lhs.wrapping_sub(rhs) as i32 as i64) as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Sllw => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                let shamt = (rhs & 0x1f) as u32;
                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    ((lhs as i32) << shamt) as i32 as i64 as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Srlw => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                let shamt = (rhs & 0x1f) as u32;
                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    ((lhs as u32) >> shamt) as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Sraw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i64;
                let rhs = self.get_x(hart_id, inst.get_r2());

                let shamt = (rhs & 0x1f) as u32;
                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    ((lhs as i32) >> shamt) as i32 as i64 as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Addi => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs.wrapping_add_signed(imm),
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Andi => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst) as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs & imm,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Ori => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst) as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs | imm,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Xori => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst) as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs ^ imm,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Slti => {

                let lhs = self.get_x(hart_id, inst.get_r1()) as i64;
                let imm = self.get_resolved_i64(hart_id, inst);

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < imm { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Sltiu => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst) as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < imm { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Slli => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = (self.get_resolved_u64(hart_id, inst) & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs << shamt,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Srli => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = (self.get_resolved_u64(hart_id, inst) & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs >> shamt,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Srai => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i64;
                let shamt = (self.get_resolved_u64(hart_id, inst) & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    (lhs >> shamt) as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Addiw => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    (lhs.wrapping_add_signed(imm) as i32 as i64) as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Slliw => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = (self.get_resolved_u64(hart_id, inst) & 0x1f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    ((lhs as i32) << shamt) as i32 as i64 as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Srliw => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = (self.get_resolved_u64(hart_id, inst) & 0x1f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    ((lhs as u32) >> shamt) as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Sraiw => {
                let lhs = self.get_x(hart_id, inst.get_r1()) as i64;
                let shamt = (self.get_resolved_u64(hart_id, inst) & 0x1f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    ((lhs as i32) >> shamt) as i32 as i64 as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Lui => {
                let imm = self.get_resolved_i64(hart_id, inst);

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    (imm << 12) as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Lb => {
                let base = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);

                let addr = base.wrapping_add_signed(imm);
                let value =  self.memory.read_i8(addr) as i64 as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Lbu => {
                let base = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);

                let addr = base.wrapping_add_signed(imm);

                let value = self.memory.read_u8(addr) as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Lh => {
                let base = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);

                let addr = base.wrapping_add_signed(imm);
                let value =  self.memory.read_i16(addr) as i64 as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Lhu => {
                let base = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);

                let addr = base.wrapping_add_signed(imm);
                let value = self.memory.read_u16(addr) as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Lw => {
                let base = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);

                let addr = base.wrapping_add_signed(imm);
                let value =  self.memory.read_i32(addr) as i64 as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Lwu => {
                let base = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);

                let addr = base.wrapping_add_signed(imm);
                let value = self.memory.read_u32(addr)  as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Ld => {
                let base = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);

                let addr = base.wrapping_add_signed(imm);
                let value = self.memory.read_u64(addr);

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Sb => {
                let value = self.get_x(hart_id, inst.get_r0());
                let base = self.get_x(hart_id, inst.get_r1());

                let imm = self.get_resolved_i64(hart_id, inst);
                let addr = base.wrapping_add_signed(imm);

                self.memory.write_u8(
                    addr,
                    value as u8,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Sh => {
                let value = self.get_x(hart_id, inst.get_r0());
                let base = self.get_x(hart_id, inst.get_r1());

                let imm = self.get_resolved_i64(hart_id, inst);
                let addr = base.wrapping_add_signed(imm);

                self.memory.write_u16(
                    addr,
                    value as u16,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Sw => {
                let value = self.get_x(hart_id, inst.get_r0());
                let base = self.get_x(hart_id, inst.get_r1());

                let imm = self.get_resolved_i64(hart_id, inst);
                let addr = base.wrapping_add_signed(imm);

                self.memory.write_u32(
                    addr,
                    value as u32,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Sd => {
                let value = self.get_x(hart_id, inst.get_r0());
                let base = self.get_x(hart_id, inst.get_r1());

                let imm = self.get_resolved_i64(hart_id, inst);
                let addr = base.wrapping_add_signed(imm);

                self.memory.write_u64(
                    addr,
                    value,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Beq => {
                let lhs = self.get_x(hart_id, inst.get_r0());
                let rhs = self.get_x(hart_id, inst.get_r1());

                if lhs == rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bne => {
                let lhs = self.get_x(hart_id, inst.get_r0());
                let rhs = self.get_x(hart_id, inst.get_r1());

                if lhs != rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Blt => {
                let lhs = self.get_x(hart_id, inst.get_r0());
                let rhs = self.get_x(hart_id, inst.get_r1());

                if lhs < rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bge => {
                let lhs = self.get_x(hart_id, inst.get_r0());
                let rhs = self.get_x(hart_id, inst.get_r1());

                if lhs >= rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bltu => {
                let lhs = self.get_x(hart_id, inst.get_r0());
                let rhs = self.get_x(hart_id, inst.get_r1());

                if lhs < rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bgeu => {
                let lhs = self.get_x(hart_id, inst.get_r0());
                let rhs = self.get_x(hart_id, inst.get_r1());

                if lhs >= rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Auipc => {
                let imm = self.get_resolved_i64(hart_id, inst) << 12;
                let pc = self.get_pc(hart_id)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    pc.wrapping_add_signed(imm),
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Jal => {
                let return_pc = self.get_pc(hart_id)? + PC_INCREMENT;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    return_pc as u64,
                );

                self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
            }
            OpCode::Jalr => {
                let base = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);
                let target = base.wrapping_add_signed(imm) as usize & !1usize;
                let return_pc = self.get_pc(hart_id)? + PC_INCREMENT;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    return_pc as u64,
                );

                self.set_pc(hart_id, Some(target))?;
            }
            OpCode::Faddd=> {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs + rhs,
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Fsubd => {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs - rhs,
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Fmuld => {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs * rhs,
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Fdivd => {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs / rhs,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Fadds => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs + rhs,
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Fsubs => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs - rhs,
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Fmuls => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs * rhs,
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Fdivs => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs / rhs,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Fsqrts => {
                let val = self.get_f32(hart_id, inst.get_r1());
                self.set_f32(hart_id, inst.get_r0(), val.sqrt());
                self.next_pc(hart_id)?;
            }
            OpCode::Fsqrtd => {
                let val = self.get_f(hart_id, inst.get_r1());
                self.set_f(hart_id, inst.get_r0(), val.sqrt());
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjs => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());
                let result = f32::from_bits((lhs.to_bits() & !(1u32 << 31)) | (rhs.to_bits() & (1u32 << 31)));
                self.set_f32(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjns => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());
                let result = f32::from_bits((lhs.to_bits() & !(1u32 << 31)) | (!rhs.to_bits() & (1u32 << 31)));
                self.set_f32(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjxs => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());
                let result = f32::from_bits(lhs.to_bits() ^ (rhs.to_bits() & (1u32 << 31)));
                self.set_f32(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjd => {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());
                let result = f64::from_bits((lhs.to_bits() & !(1u64 << 63)) | (rhs.to_bits() & (1u64 << 63)));
                self.set_f(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjnd => {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());
                let result = f64::from_bits((lhs.to_bits() & !(1u64 << 63)) | (!rhs.to_bits() & (1u64 << 63)));
                self.set_f(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjxd => {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());
                let result = f64::from_bits(lhs.to_bits() ^ (rhs.to_bits() & (1u64 << 63)));
                self.set_f(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fmins => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());
                self.set_f32(hart_id, inst.get_r0(), lhs.min(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Fmaxs => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());
                self.set_f32(hart_id, inst.get_r0(), lhs.max(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Fmind => {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());
                self.set_f(hart_id, inst.get_r0(), lhs.min(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Fmaxd => {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());
                self.set_f(hart_id, inst.get_r0(), lhs.max(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtsd => {
                let val = self.get_f32(hart_id, inst.get_r1()) as f64;
                self.set_f(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtds => {
                let val = self.get_f(hart_id, inst.get_r1()) as f32;
                self.set_f32(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtws => {
                let val = self.get_f32(hart_id, inst.get_r1()) as i32 as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtwus => {
                let val = self.get_f32(hart_id, inst.get_r1());
                let result = (val as u32) as u64;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtwd => {
                let val = self.get_f(hart_id, inst.get_r1()) as i32 as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtwud => {
                let val = self.get_f(hart_id, inst.get_r1());
                let result = (val as u32) as u64;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtsw => {
                let val = self.get_x(hart_id, inst.get_r1()) as i32 as f32;
                self.set_f32(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtswu => {
                let val = self.get_x(hart_id, inst.get_r1()) as u32 as f32;
                self.set_f32(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtdw => {
                let val = self.get_x(hart_id, inst.get_r1()) as i32 as f64;
                self.set_f(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtdwu => {
                let val = self.get_x(hart_id, inst.get_r1()) as u32 as f64;
                self.set_f(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtls => {
                let val = self.get_f32(hart_id, inst.get_r1()) as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtlus => {
                let val = self.get_f32(hart_id, inst.get_r1()) as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtld => {
                let val = self.get_f(hart_id, inst.get_r1()) as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtlud => {
                let val = self.get_f(hart_id, inst.get_r1()) as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtsl => {
                let val = self.get_x(hart_id, inst.get_r1()) as i64 as f32;
                self.set_f32(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtslu => {
                let val = self.get_x(hart_id, inst.get_r1()) as f32;
                self.set_f32(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtdl => {
                let val = self.get_x(hart_id, inst.get_r1()) as i64 as f64;
                self.set_f(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtdlu => {
                let val = self.get_x(hart_id, inst.get_r1()) as f64;
                self.set_f(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fclasss => {
                let val = self.get_f32(hart_id, inst.get_r1());
                let bits = val.to_bits();
                let result: u64 = if val.is_infinite() {
                    if bits & (1u32 << 31) != 0 { 1 << 0 } else { 1 << 7 }
                } else if val.is_nan() {
                    if bits & (1 << 22) != 0 { 1 << 8 } else { 1 << 9 }
                } else if val == 0.0 {
                    if bits & (1u32 << 31) != 0 { 1 << 3 } else { 1 << 4 }
                } else if val.is_subnormal() {
                    if bits & (1u32 << 31) != 0 { 1 << 2 } else { 1 << 5 }
                } else {
                    if bits & (1u32 << 31) != 0 { 1 << 1 } else { 1 << 6 }
                };
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fclassd => {
                let val = self.get_f(hart_id, inst.get_r1());
                let bits = val.to_bits();
                let result: u64 = if val.is_infinite() {
                    if bits & (1u64 << 63) != 0 { 1 << 0 } else { 1 << 7 }
                } else if val.is_nan() {
                    if bits & (1u64 << 51) != 0 { 1 << 8 } else { 1 << 9 }
                } else if val == 0.0 {
                    if bits & (1u64 << 63) != 0 { 1 << 3 } else { 1 << 4 }
                } else if val.is_subnormal() {
                    if bits & (1u64 << 63) != 0 { 1 << 2 } else { 1 << 5 }
                } else {
                    if bits & (1u64 << 63) != 0 { 1 << 1 } else { 1 << 6 }
                };
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fmvxw => {
                let val = self.get_f32(hart_id, inst.get_r1());
                self.set_x(hart_id, inst.get_r0(), val.to_bits() as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Fmvwx => {
                let val = self.get_x(hart_id, inst.get_r1()) as u32;
                self.set_f32(hart_id, inst.get_r0(), f32::from_bits(val));
                self.next_pc(hart_id)?;
            }
            OpCode::Feqd => {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs == rhs { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Fltd => {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < rhs { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Fled => {
                let lhs = self.get_f(hart_id, inst.get_r1());
                let rhs = self.get_f(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs <= rhs { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Feqs => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs == rhs { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Flts => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < rhs { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Fles => {
                let lhs = self.get_f32(hart_id, inst.get_r1());
                let rhs = self.get_f32(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs <= rhs { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Fmvdx => {
                let value = self.get_x(hart_id, inst.get_r1());
                self.set_f_bits(hart_id,inst.get_r0(), value);
                self.next_pc(hart_id)?;
            }
            OpCode::Fmvxd => {
                let value = self.get_f_bits(hart_id, inst.get_r1());
                self.set_x(hart_id, inst.get_r0(), value);
                self.next_pc(hart_id)?;
            }
            OpCode::Fld => {
                let base = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);
                let addr = base.wrapping_add_signed(imm);
                let bits = self.memory.read_u64(addr);

                self.set_f_bits(
                    hart_id,
                    inst.get_r0(),
                    bits,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Fsd => {
                let bits = self.get_f_bits(hart_id, inst.get_r0());
                let base = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_resolved_i64(hart_id, inst);
                let addr = base.wrapping_add_signed(imm);

                self.memory.write_u64(addr,bits);

                self.next_pc(hart_id)?;
            }
            _ => {
                return Err(DebuggerError::GeneralError(format!("unsupported instruction: {:?}", opcode)));
            }
        }

        Ok(())
    }

    /// get u64 value from Imm, if Imm is None, return 0
    fn get_u64_from_imm(&self, hart_id: HartId, imm: Option<&Imm>) -> u64 {
        match imm {
            Some(Imm::Value(s)) => core_utils::number::get_u64_from_str(s).unwrap_or(0),
            Some(Imm::ImmMacro(n)) => {
                match n {
                    riscv_asm_lib::r5asm::imm_macro::ImmMacro::PtrSize => 
                        self.get_processor_from_hart_id(hart_id).unwrap().addressing.to_ptr_size(),
                }
            }
            None => 0,
        }
    }

    /// get i64 value from Imm, if Imm is None, return 0
    fn get_i64_from_imm(&self, hart_id: HartId, imm: Option<&Imm>) -> i64 {
        match imm {
            Some(Imm::Value(s)) => core_utils::number::get_i64_from_str(s).unwrap_or(0),
            Some(Imm::ImmMacro(n)) => {
                match n {
                    riscv_asm_lib::r5asm::imm_macro::ImmMacro::PtrSize => 
                        self.get_processor_from_hart_id(hart_id).unwrap().addressing.to_ptr_size() as i64,
                }
            }
            None => 0,
        }
    }

    fn next_pc(&mut self, hart_id: HartId) -> Result<(), DebuggerError> {
        let err_msg = format!("invalid hart: {}", hart_id);
        let hart = self.get_hart_mut(hart_id).ok_or_else(|| DebuggerError::GeneralError(err_msg))?;
        hart.next_pc();
        Ok(())
    }

    fn set_pc(&mut self, hart_id: HartId, pc: Option<usize>) -> Result<(), DebuggerError> {
        let err_msg = format!("invalid hart: {}", hart_id);
        let hart = self.get_hart_mut(hart_id).ok_or_else(|| DebuggerError::GeneralError(err_msg))?;
        hart.pc = pc.unwrap_or(hart.pc);
        Ok(())
    }

    fn get_pc(&self, hart_id: HartId) -> Result<usize, DebuggerError> {
        let err_msg = format!("invalid hart: {}", hart_id);
        let hart = self.get_hart(hart_id).ok_or_else(|| DebuggerError::GeneralError(err_msg))?;
        Ok(hart.pc)
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