
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

use std::collections::HashMap;

pub type ProcessorId = usize;
pub type HartId = u64;
pub type ProgramId = usize;

pub struct Machine {
    pub processors: Vec<Processor>,

    pub programs: Vec<AsmProgram>,

    pub memory: Memory,

    pub registers: Register,

    /// O(1) cache: (program_id, pc_offset) → index in text_section_items
    inst_cache: HashMap<(ProgramId, usize), usize>,
}

impl Machine {

    pub fn new() -> Self {
        let mut r = Self {
            processors: vec![],
            programs: vec![],
            memory: Memory::default(),
            registers: Register::new(),
            inst_cache: HashMap::new(),
        };

        r.add_processor(Processor::default());
        r
    }

    pub fn add_program(&mut self, program: AsmProgram) -> Result<ProgramId, DebuggerError> {
        let id = self.programs.len();
        self.load_program_memory(&program)?;

        // Build O(1) instruction cache for fast PC→instruction lookup
        for (idx, item) in program.get_text_section_items().iter().enumerate() {
            if item.get_inc().is_some() {
                self.inst_cache.insert((id, item.get_offset()), idx);
            }
        }

        self.programs.push(program);

        Ok(id)
    }

    fn load_program_memory(&mut self, program: &AsmProgram) -> Result<(), DebuggerError> {
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

            self.memory.write_bytes(item.get_offset() as u64, &bytes)?;
        }
        Ok(())
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
        let hart = self.get_hart_mut(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        // Reset all register files to default for clean restart
        hart.x = crate::machine::hart::IntegerRegisterFile::default();
        hart.f = crate::machine::hart::FloatRegisterFile::default();
        hart.v = crate::machine::hart::VectorRegisterFile::default();
        hart.vector_state = crate::machine::hart::VectorState::default();
        hart.csr = crate::machine::hart::CsrFile::default();
        hart.pc = entry_point;
        Ok(())
    }

    /// fetch instruction for given hart, O(1) lookup via cache
    fn fetch_inst(&self, hart: &Hart) -> Option<&Instruction> {
        let idx = self.inst_cache.get(&(hart.program_id, hart.pc))?;
        let program = &self.programs[hart.program_id];
        program.get_text_section_items()[*idx].get_inc()
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
            let hart = self.get_hart(hart_id)
                                    .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;

            (hart.program_id, hart.pc)
        };

        // O(1) lookup via cache instead of linear scan
        let inst = self.inst_cache
            .get(&(program_id, pc))
            .and_then(|idx| {
                let prog = &self.programs[program_id];
                prog.get_text_section_items()[*idx].get_inc().cloned()
            });

        if let Some(inst) = inst {
            // Per-instruction detail log at debug level — avoids flooding during Continue
            let opcode = inst.get_op_code().map(|o| format!("{:?}", o)).unwrap_or_else(|_| "unknown".into());
            log::debug!("EXEC: pc={:x} op={} mnemonic={:?} r0={:?} r1={:?} r2={:?} imm={:?}",
                pc, opcode, inst.get_name(), inst.get_r0(), inst.get_r1(), inst.get_r2(), inst.get_imm());
            let result = self.execute_inst(hart_id, &inst);
            // Per-instruction register log at debug level — avoids flooding during Continue
            if let Some(hart) = self.get_hart(hart_id) {
                log::debug!("POST: t1(x6)={:x} t2(x7)={:x} t0(x5)={:x} a0(x10)={:x} a1(x11)={:x} a2(x12)={:x} a3(x13)={:x}",
                    hart.x.read(6), hart.x.read(7), hart.x.read(5),
                    hart.x.read(10), hart.x.read(11), hart.x.read(12),
                    hart.x.read(13));
            }
            result
        }
        else {
            let prog = &self.programs[program_id];
            if prog.get_text_section_items().is_empty() {
                return Err(DebuggerError::GeneralError(format!("no instructions found in program id: {}", program_id)));
            }

            Err(DebuggerError::GeneralError(format!("no instruction found at pc: {}", pc)))
        }
    }

    /// Resolve register name to index, returning error instead of panicking
    fn resolve_reg_index(&self, reg: Option<&String>) -> Result<usize, DebuggerError> {
        self.registers
            .get_register_value(reg)
            .map(|v| v as usize)
            .map_err(|_| DebuggerError::RegisterReadError(
                format!("unknown register: {:?}", reg)
            ))
    }

    fn xreg(
        &self,
        name: &Option<String>,
    ) -> Result<usize, DebuggerError> {
        self.resolve_reg_index(name.as_ref())
    }

    fn get_f32(&self, hart_id: HartId, reg: Option<&String>) -> Result<f32, DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        Ok(hart.f.regs[idx].value as f32)
    }

    fn set_f32(&mut self, hart_id: HartId, reg: Option<&String>, value: f32) -> Result<(), DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart_mut(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        hart.f.regs[idx].value = value as f64;
        Ok(())
    }

    fn get_f(&self, hart_id: HartId, reg: Option<&String>) -> Result<f64, DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        Ok(hart.f.regs[idx].value)
    }

    pub fn get_f_bits(&self, hart_id: HartId, reg: Option<&String>) -> Result<u64, DebuggerError> {
        self.get_f(hart_id, reg)
            .map(|v| v.to_bits())
    }

    pub fn set_f_bits(&mut self, hart_id: HartId, reg: Option<&String>, bits: u64) -> Result<(), DebuggerError> {
        self.set_f(hart_id, reg, f64::from_bits(bits))
    }

    fn set_f(&mut self, hart_id: HartId, reg: Option<&String>, value: f64) -> Result<(), DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart_mut(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        hart.f.regs[idx].value = value;
        Ok(())
    }

    fn get_x(&self, hart_id: HartId, reg: Option<&String>) -> Result<u64, DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        Ok(hart.x.read(idx))
    }

    fn set_x(&mut self, hart_id: HartId, reg: Option<&String>, value: u64) -> Result<(), DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart_mut(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        hart.x.write(idx, value); // IntegerRegisterFile::write guards x0
        Ok(())
    }

    fn get_resolved_u64(&self, hart_id: HartId, inst: &Instruction) -> Result<u64, DebuggerError> {
        if inst.get_rel_fun().is_some() {
            Ok(inst.get_virtual_address() as u64)
        } else {
            self.get_u64_from_imm(hart_id, inst.get_imm())
        }
    }

    fn get_resolved_i64(&self, hart_id: HartId, inst: &Instruction) -> Result<i64, DebuggerError> {
        if inst.get_rel_fun().is_some() {
            Ok(inst.get_virtual_address() as i32 as i64)
        } else {
            self.get_i64_from_imm(hart_id, inst.get_imm())
        }
    }

    fn binary_operation(&mut self, hart_id: HartId, inst: &Instruction, op: impl Fn(u64, u64) -> u64) -> Result<(), DebuggerError> {
        let lhs = self.get_x(hart_id, inst.get_r1())?;
        let rhs = self.get_x(hart_id, inst.get_r2())?;
        self.set_x(hart_id, inst.get_r0(), op(lhs, rhs))?;
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
            OpCode::Sra => self.binary_operation(hart_id, &inst, |a, b| ((a as i64) >> (b & 0x3f) as u32) as u64)?,
            OpCode::Slt  => self.binary_operation(hart_id, &inst, |a, b| ((a as i64) < (b as i64)) as u64)?,
            OpCode::Sltu => self.binary_operation(hart_id, &inst, |a, b| (a < b) as u64)?,
            OpCode::Mul  => self.binary_operation(hart_id, &inst, |a, b| a.wrapping_mul(b))?,
            OpCode::Addi => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs.wrapping_add_signed(imm),
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Andi => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs & imm,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Ori => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs | imm,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Xori => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs ^ imm,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Slti => {

                let lhs = self.get_x(hart_id, inst.get_r1())? as i64;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < imm { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Sltiu => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < imm { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Slli => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let shamt = (self.get_resolved_u64(hart_id, inst)? & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs << shamt,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Srli => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let shamt = (self.get_resolved_u64(hart_id, inst)? & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs >> shamt,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Srai => {
                let lhs = self.get_x(hart_id, inst.get_r1())? as i64;
                let shamt = (self.get_resolved_u64(hart_id, inst)? & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    (lhs >> shamt) as u64,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lui => {
                let imm = self.get_resolved_i64(hart_id, inst)?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    (imm << 12) as u64,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lb => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value =  self.memory.read_i8(addr)? as i64 as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lbu => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);

                let value = self.memory.read_u8(addr)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lh => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value =  self.memory.read_i16(addr)? as i64 as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lhu => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value = self.memory.read_u16(addr)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lw => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value =  self.memory.read_i32(addr)? as i64 as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lwu => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value = self.memory.read_u32(addr)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Ld => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value = self.memory.read_u64(addr)?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Sb => {
                let value = self.get_x(hart_id, inst.get_r0())?;
                let base = self.get_x(hart_id, inst.get_r1())?;

                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);

                self.memory.write_u8(
                    addr,
                    value as u8,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Sh => {
                let value = self.get_x(hart_id, inst.get_r0())?;
                let base = self.get_x(hart_id, inst.get_r1())?;

                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);

                self.memory.write_u16(
                    addr,
                    value as u16,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Sw => {
                let value = self.get_x(hart_id, inst.get_r0())?;
                let base = self.get_x(hart_id, inst.get_r1())?;

                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);

                self.memory.write_u32(
                    addr,
                    value as u32,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Sd => {
                let value = self.get_x(hart_id, inst.get_r0())?;
                let base = self.get_x(hart_id, inst.get_r1())?;

                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);

                self.memory.write_u64(
                    addr,
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Beq => {
                let lhs = self.get_x(hart_id, inst.get_r0())?;
                let rhs = self.get_x(hart_id, inst.get_r1())?;

                if lhs == rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bne => {
                let lhs = self.get_x(hart_id, inst.get_r0())?;
                let rhs = self.get_x(hart_id, inst.get_r1())?;

                if lhs != rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Blt => {
                let lhs = self.get_x(hart_id, inst.get_r0())? as i64;
                let rhs = self.get_x(hart_id, inst.get_r1())? as i64;

                if lhs < rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bge => {
                let lhs = self.get_x(hart_id, inst.get_r0())? as i64;
                let rhs = self.get_x(hart_id, inst.get_r1())? as i64;

                if lhs >= rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bltu => {
                let lhs = self.get_x(hart_id, inst.get_r0())?;
                let rhs = self.get_x(hart_id, inst.get_r1())?;

                if lhs < rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bgeu => {
                let lhs = self.get_x(hart_id, inst.get_r0())?;
                let rhs = self.get_x(hart_id, inst.get_r1())?;

                if lhs >= rhs {
                    self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Auipc => {
                let imm = self.get_resolved_i64(hart_id, inst)? << 12;
                let pc = self.get_pc(hart_id)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    pc.wrapping_add_signed(imm),
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Jal => {
                let return_pc = self.get_pc(hart_id)? + PC_INCREMENT;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    return_pc as u64,
                )?;

                self.set_pc(hart_id,  self.get_inst_target(hart_id))?;
            }
            OpCode::Jalr => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;
                let target = base.wrapping_add_signed(imm) as usize & !1usize;
                let return_pc = self.get_pc(hart_id)? + PC_INCREMENT;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    return_pc as u64,
                )?;

                self.set_pc(hart_id, Some(target))?;
            }
            OpCode::Faddd=> {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs + rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fsubd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs - rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fmuld => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs * rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fdivd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs / rhs,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Fadds => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs + rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fsubs => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs - rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fmuls => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs * rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fdivs => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs / rhs,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Feqd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs == rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fltd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fled => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs <= rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Feqs => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs == rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Flts => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fles => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs <= rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Fmvdx => {
                let value = self.get_x(hart_id, inst.get_r1())?;
                self.set_f_bits(hart_id, inst.get_r0(), value)?;
                self.next_pc(hart_id)?;
            }
            OpCode::Fmvxd => {
                let value = self.get_f_bits(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), value)?;
                self.next_pc(hart_id)?;
            }
            OpCode::Fld => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);
                let bits = self.memory.read_u64(addr)?;

                self.set_f_bits(
                    hart_id,
                    inst.get_r0(),
                    bits,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Fsd => {
                let bits = self.get_f_bits(hart_id, inst.get_r0())?;
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);

                self.memory.write_u64(addr,bits)?;

                self.next_pc(hart_id)?;
            }
            _ => {
                return Err(DebuggerError::GeneralError(format!("unsupported instruction: {:?}", opcode)));
            }
        }

        Ok(())
    }

    /// get u64 value from Imm, if Imm is None, return 0
    fn get_u64_from_imm(&self, hart_id: HartId, imm: Option<&Imm>) -> Result<u64, DebuggerError> {
        match imm {
            Some(Imm::Value(s)) => core_utils::number::get_u64_from_str(s)
                .map_err(|e| DebuggerError::GeneralError(format!("failed to parse immediate '{}' as u64: {:?}", s, e))),
            Some(Imm::ImmMacro(n)) => {
                match n {
                    riscv_asm_lib::r5asm::imm_macro::ImmMacro::PtrSize => 
                        self.get_processor_from_hart_id(hart_id)
                            .map(|p| p.addressing.to_ptr_size())
                            .ok_or_else(|| DebuggerError::GeneralError("failed to resolve PtrSize for hart".to_string())),
                }
            }
            None => Ok(0),
        }
    }

    /// get i64 value from Imm, if Imm is None, return 0
    fn get_i64_from_imm(&self, hart_id: HartId, imm: Option<&Imm>) -> Result<i64, DebuggerError> {
        match imm {
            Some(Imm::Value(s)) => core_utils::number::get_i64_from_str(s)
                .map_err(|e| DebuggerError::GeneralError(format!("failed to parse immediate '{}' as i64: {:?}", s, e))),
            Some(Imm::ImmMacro(n)) => {
                match n {
                    riscv_asm_lib::r5asm::imm_macro::ImmMacro::PtrSize => 
                        self.get_processor_from_hart_id(hart_id)
                            .map(|p| p.addressing.to_ptr_size() as i64)
                            .ok_or_else(|| DebuggerError::GeneralError("failed to resolve PtrSize for hart".to_string())),
                }
            }
            None => Ok(0),
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
