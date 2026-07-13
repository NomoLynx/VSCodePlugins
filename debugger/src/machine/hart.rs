use crate::debugger_error::DebuggerError;
use crate::machine::{machine::ProgramId, register_ref::{RegisterRef, RegisterType}, runtime_value::RuntimeValue};

#[derive(Clone, Debug)]
pub struct RegValue<T> {
    pub value: T,
    pub provenance: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct IntegerRegisterFile {
    pub regs: Vec<RegValue<u64>>,
}

impl IntegerRegisterFile {
    pub fn read(&self, reg: usize) -> u64 {
        if reg == 0 {
            return 0; // x0 is hardwired to 0
        }
        if reg >= self.regs.len() {
            log::error!(
                "IntegerRegisterFile::read: index {} out of bounds (max {}) — returning 0 (this masks a bug in the caller)",
                reg,
                self.regs.len().saturating_sub(1)
            );
            return 0;
        }
        self.regs[reg].value
    }

    pub fn write(&mut self, reg: usize, value: u64) {
        if reg == 0 {
            return; // x0 is hardwired to 0
        }
        if reg >= self.regs.len() {
            log::error!(
                "IntegerRegisterFile::write: index {} out of bounds (max {}) — write dropped (this masks a bug in the caller)",
                reg,
                self.regs.len().saturating_sub(1)
            );
            return;
        }
        self.regs[reg].value = value;
    }

    pub fn new(size:usize) -> Self {
        Self {
            regs: vec![RegValue { value: 0, provenance: None }; size],
        }
    }
}

impl Default for IntegerRegisterFile {
    fn default() -> Self {
        Self::new(32)
    }
}

/// Float register file stores raw 64-bit patterns.
/// All single/double-precision ops interpret bits on-the-fly.
/// Single-precision writes NaN-box bits[63:32]=0xFFFFFFFF per RISC-V spec.
#[derive(Clone, Debug)]
pub struct FloatRegisterFile {
    pub regs: Vec<RegValue<u64>>,
}

impl FloatRegisterFile {
    pub fn read(&self, reg: usize) -> u64 {
        if reg >= self.regs.len() {
            log::error!(
                "FloatRegisterFile::read: index {} out of bounds (max {}) — returning 0 (this masks a bug in the caller)",
                reg,
                self.regs.len().saturating_sub(1)
            );
            return 0;
        }
        self.regs[reg].value
    }

    pub fn write(&mut self, reg: usize, value: u64) {
        if reg >= self.regs.len() {
            log::error!(
                "FloatRegisterFile::write: index {} out of bounds (max {}) — write dropped (this masks a bug in the caller)",
                reg,
                self.regs.len().saturating_sub(1)
            );
            return;
        }
        self.regs[reg].value = value;
    }

    pub fn new(size:usize) -> Self {
        Self {
            regs: vec![RegValue { value: 0, provenance: None }; size],
        }
    }
}

impl Default for FloatRegisterFile {
    fn default() -> Self {
        Self::new(32)
    }
}

#[derive(Clone, Debug)]
pub struct VectorRegister {
    pub bytes: Vec<u8>,
}

impl Default for VectorRegister {
    fn default() -> Self {
        Self {
            bytes: vec![0; 64], // 512 bits = 64 bytes
        }
    }
}

#[derive(Clone, Debug)]
pub struct VectorRegisterFile {
    pub regs: Vec<VectorRegister>,
}

impl VectorRegisterFile {
    pub fn read_bytes(&self, reg: usize) -> Option<Vec<u8>> {
        self.regs.get(reg).map(|vr| vr.bytes.clone())
    }

    pub fn write_bytes(&mut self, reg: usize, data: &[u8]) -> bool {
        if let Some(vr) = self.regs.get_mut(reg) {
            let len = data.len().min(vr.bytes.len());
            vr.bytes[..len].copy_from_slice(&data[..len]);
            true
        } else {
            false
        }
    }
}

impl Default for VectorRegisterFile {
    fn default() -> Self {
        Self {
            regs: vec![VectorRegister::default(); 32],
        }
    }
}

#[derive(Clone, Debug)]
pub struct CsrFile {
    pub mhartid: u64,
    pub vl: u64,
    pub vtype: u64,
}

impl Default for CsrFile {
    fn default() -> Self {
        Self {
            mhartid: 0,
            vl: 0,
            vtype: 0,
        }
    }
}

/// RISC-V standard CSR addresses used for register index mapping.
pub const CSR_MHARTID: usize = 0xF14;
pub const CSR_VL:      usize = 0xC20;
pub const CSR_VTYPE:   usize = 0xC21;

/// Runtime state of a hardware thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HartState {
    /// Hart is actively executing instructions.
    Running,
    /// Hart is temporarily suspended (e.g. by debugger).
    Halted,
    /// Hart has no more instructions to execute.
    Finished,
}

/// Unified performance snapshot for a single hart.
/// All cycle counts are derived from the shared global clock.
#[derive(Clone, Debug)]
pub struct HartPerformance {
    pub hart_id: u64,
    pub processor_id: usize,
    pub state: HartState,
    /// Total global clock cycles this hart has experienced (time elapsed).
    /// Always synchronized with the shared global clock in parallel mode.
    pub elapsed_cycles: u64,
    /// Total execution cycles consumed by instructions executed on this hart.
    /// Used for IPC calculation; grows only when an instruction actually executes.
    pub exec_cycles: u64,
    /// Total instructions executed by this hart.
    pub inst_count: u64,
    /// Runtime cycles since this hart was created (global_clock - start_clock).
    pub runtime_cycles: u64,
}

impl HartPerformance {
    /// Instructions per cycle — based on actual execution cycles.
    pub fn ipc(&self) -> f64 {
        if self.exec_cycles > 0 {
            self.inst_count as f64 / self.exec_cycles as f64
        } else {
            0.0
        }
    }
}

pub const PC_INCREMENT: usize = 4;
#[derive(Clone, Debug)]
pub struct Hart {
    pub id: u64,
    pub pc: usize,
    pub program_id: ProgramId,
    pub x: IntegerRegisterFile,
    pub f: FloatRegisterFile,
    pub v: VectorRegisterFile,
    pub csr: CsrFile,
    /// Global clock cycles this hart has experienced (time elapsed).
    /// Always synchronized with the shared global clock in parallel mode.
    pub elapsed_cycles: u64,
    /// Total execution cycles consumed by instructions executed on this hart.
    /// Used for IPC calculation; grows only when an instruction actually executes.
    pub exec_cycles: u64,
    /// Number of instructions executed by this hart.
    pub inst_count: u64,
    /// global_clock value at the moment this hart was created / re-initialized.
    pub start_clock: u64,
    /// Current runtime state of this hart.
    pub state: HartState,
    /// Processor clock value at the moment this hart transitioned to Finished.
    /// Used to freeze runtime_cycles at the actual end of execution, preventing
    /// it from growing indefinitely after the program terminates.
    pub finish_clock: Option<u64>,
    /// If the hart terminated due to an execution error (illegal instruction,
    /// memory access fault, etc.), this holds the error description.
    /// None means normal termination (PC past end of text) or still Running.
    pub error_info: Option<String>,
}

impl Hart {
    pub fn new(id: u64, program_id: ProgramId) -> Self {
        Self {
            id,
            pc: 0,
            program_id,
            x: IntegerRegisterFile::default(),
            f: FloatRegisterFile::default(),
            v: VectorRegisterFile::default(),
            csr: CsrFile::default(),
            elapsed_cycles: 0,
            exec_cycles: 0,
            inst_count: 0,
            start_clock: 0,
            state: HartState::Running,
            finish_clock: None,
            error_info: None,
        }
    }

    /// increment the program counter to point to the next instruction
    pub fn next_pc(&mut self) {
        self.pc += PC_INCREMENT;
    }

    /// set the program counter to a specific value
    pub fn set_pc(&mut self, pc: usize) {
        self.pc = pc;
    }

    pub fn read_register(&self, reg: &RegisterRef) -> RuntimeValue {

        match reg.reg_type {

            RegisterType::Integer => {

                RuntimeValue::Integer(
                    self.x.read(reg.index)
                )
            }

            RegisterType::Float => {
                let bits = self.f.read(reg.index);
                // P0-7/P0-11: Detect RISC-V NaN-boxing — single-precision
                // values have upper 32 bits set to 0xFFFFFFFF, which would
                // be parsed as NaN by f64::from_bits.  When NaN-boxed,
                // interpret the low 32 bits as f32 instead.
                if (bits >> 32) == 0xFFFFFFFF {
                    RuntimeValue::Float32(f32::from_bits(bits as u32))
                } else {
                    RuntimeValue::Float64(f64::from_bits(bits))
                }
            }

            RegisterType::Vector => {
                RuntimeValue::Vector(
                    self.v.read_bytes(reg.index)
                        .unwrap_or_default()
                )
            }

            RegisterType::Csr => {
                match reg.index {
                    CSR_MHARTID => RuntimeValue::Integer(self.csr.mhartid),
                    CSR_VL      => RuntimeValue::Integer(self.csr.vl),
                    CSR_VTYPE   => RuntimeValue::Integer(self.csr.vtype),
                    _ => RuntimeValue::Unavailable,
                }
            }
        }
    }

    pub fn write_register(
        &mut self,
        reg: &RegisterRef,
        value: RuntimeValue,
    ) -> Result<(), DebuggerError> {

        let reg_type = reg.reg_type;
        match (reg_type, value) {
            (RegisterType::Integer, RuntimeValue::Integer(v)) => {
                self.x.write(reg.index, v); // IntegerRegisterFile::write guards x0
            }

            (RegisterType::Float, RuntimeValue::Float64(v)) => {
                // P0-16: Write the full 64-bit value directly.  NaN-boxing
                // (setting high 32 bits to 0xFFFFFFFF for single-precision
                // values) is the instruction-execution layer's responsibility,
                // not the debugger's.  Basing the boxing decision on the
                // register's old state would truncate any f64 value that
                // happened to follow an NaN-boxed f32, corrupting the value.
                self.f.write(reg.index, v.to_bits());
            }

            (RegisterType::Vector, RuntimeValue::Vector(v)) => {
                if !self.v.write_bytes(reg.index, &v) {
                    return Err(DebuggerError::RegisterWriteError(
                        format!("vector register index {} out of range (max 31)", reg.index)
                    ));
                }
            }

            (RegisterType::Csr, RuntimeValue::Integer(v)) => {
                match reg.index {
                    // P0-5: mhartid is a read-only hardware thread ID register
                    // per RISC-V privileged spec §3.1.1. Allowing writes would
                    // corrupt hart identity in multi-hart scenarios (thread
                    // ordering, performance counters, breakpoint maps, etc.).
                    CSR_MHARTID | CSR_VL | CSR_VTYPE => {
                        let name = match reg.index {
                            CSR_MHARTID => "mhartid",
                            CSR_VL => "vl",
                            CSR_VTYPE => "vtype",
                            _ => unreachable!(),
                        };
                        return Err(DebuggerError::RegisterWriteError(
                            format!("{} is a read-only CSR per RISC-V spec", name)
                        ));
                    }
                    _ => return Err(DebuggerError::RegisterWriteError(
                        format!("unsupported CSR index: {:#x}", reg.index)
                    )),
                }
            }

            (unexpected_type, unexpected_value) => {
                return Err(DebuggerError::RegisterWriteError(
                    format!("register type mismatch: expected {:?}, got {:?}", unexpected_type, unexpected_value)
                ));
            }
        }

        Ok(())
    }
}

impl Default for Hart {
    fn default() -> Self {
        Self {
            id: 0,
            pc: 0,
            program_id: 0,
            x: IntegerRegisterFile::default(),
            f: FloatRegisterFile::default(),
            v: VectorRegisterFile::default(),
            csr: CsrFile::default(),
            elapsed_cycles: 0,
            exec_cycles: 0,
            inst_count: 0,
            start_clock: 0,
            state: HartState::Running,
            finish_clock: None,
            error_info: None,
        }
    }
}
