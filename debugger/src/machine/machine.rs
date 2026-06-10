use riscv_asm_lib::r5asm::asm_program::AsmProgram;
use riscv_asm_lib::r5asm::imm::Imm::{self};
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

    pub fn get_default_hart(&self) -> Option<&Hart> {
        self.get_hart(0)
    }

    pub fn get_default_hart_mut(&mut self) -> Option<&mut Hart> {
        self.get_hart_mut(0)
    }

    /// fetch instruction for given hart, return None if no instruction found (e.g. pc is out of range)
    fn fetch_inst(&self, hart: &Hart) -> Option<&Instruction> {
        let program = &self.programs[hart.program_id];
        let text_items = program.get_text_section_items();
        let item = text_items.get(hart.pc)
                                            .and_then(|x| x.get_inc());
        item
    }

    /// get instruction offset for given hart, return None if no instruction found (e.g. pc is out of range)
    fn get_inst_offset(&self, hart: &Hart) -> Option<usize> {
        let program = &self.programs[hart.program_id];
        let text_items = program.get_text_section_items();
        let item = text_items.get(hart.pc);
        item.map(|x| x.get_offset())
    }

    /// get instruction target address for given hart, 
    /// return None if no instruction found (e.g. pc is out of range)
    fn get_inst_target(&self, hart_id: HartId) -> Option<usize> {
        let hart = self.get_hart(hart_id)?;
        let inst = self.fetch_inst(hart)?;
        let inst_offset = self.get_inst_offset(hart)?;
        let target = self.get_i64_from_imm(hart_id, inst.get_imm());
        Some((inst_offset as i64).wrapping_add(target) as usize)
    }

    pub fn step_hart(&mut self, hart_id: HartId) -> Result<(), DebuggerError> {

        let (program_id, pc) = {
            let hart = self.get_hart_mut(hart_id)
                                    .expect("invalid hart");

            (hart.program_id, hart.pc)
        };

        if let Some(inst) = 
            self.programs[program_id]
                .get_text_section_items()
                .get(pc)
                .and_then(|x| x.get_inc())
                .cloned() {

            self.execute_inst(hart_id, &inst)
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
                self.set_x(hart_id, inst.get_r0(),lhs.wrapping_add(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Sub => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs.wrapping_sub(rhs),
                );

                self.next_pc(hart_id)?;
            }

            OpCode::And => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs & rhs,
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Or => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs | rhs,
                );

                self.next_pc(hart_id)?;
            }

            OpCode::Xor => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs ^ rhs,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Sll => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                let shamt = (rhs & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs << shamt,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Srl => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let rhs = self.get_x(hart_id, inst.get_r2());

                let shamt = (rhs & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs >> shamt,
                );

                self.next_pc(hart_id)?;
            }
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
            OpCode::Addi => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm()) as i64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs.wrapping_add_signed(imm),
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Andi => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let imm =self.get_u64_from_imm(hart_id, inst.get_imm());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs & imm,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Ori => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let imm =self.get_u64_from_imm(hart_id, inst.get_imm());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs | imm,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Xori => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let imm =self.get_u64_from_imm(hart_id, inst.get_imm());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs ^ imm,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Slti => {

                let lhs = self.get_x(hart_id, inst.get_r1()) as i64;
                let imm =self.get_u64_from_imm(hart_id, inst.get_imm()) as i64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < imm { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Sltiu => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < imm { 1 } else { 0 },
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Slli => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = (self.get_u64_from_imm(hart_id, inst.get_imm()) as u64 & 0x3f)  as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs << shamt,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Srli => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = (self.get_u64_from_imm(hart_id, inst.get_imm()) as u64 & 0x3f)  as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs >> shamt,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Srai => {
                let lhs = self.get_x(hart_id, inst.get_r1());
                let shamt = (self.get_u64_from_imm(hart_id, inst.get_imm()) as u64 & 0x3f)  as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    (lhs >> shamt) as u64,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Lui => {
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm());

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    imm << 12,
                );

                self.next_pc(hart_id)?;
            }
            OpCode::Lb => {
                let base = self.get_x(hart_id, inst.get_r1());
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm());

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
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm());

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
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm());

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
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm());

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
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm());

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
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm());

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
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm());

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

                let imm = self.get_i64_from_imm(hart_id, inst.get_imm());
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

                let imm = self.get_i64_from_imm(hart_id, inst.get_imm());
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

                let imm = self.get_i64_from_imm(hart_id, inst.get_imm());
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

                let imm = self.get_i64_from_imm(hart_id, inst.get_imm());
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