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
                "IntegerRegisterFile::read: index {} out of bounds (max {}) returning 0 (this masks a bug in the caller)",
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
                "IntegerRegisterFile::write: index {} out of bounds (max {}) write dropped (this masks a bug in the caller)",
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
                "FloatRegisterFile::read: index {} out of bounds (max {}) returning 0 (this masks a bug in the caller)",
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
                "FloatRegisterFile::write: index {} out of bounds (max {}) write dropped (this masks a bug in the caller)",
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
    // Machine Information Registers
    pub mhartid: u64,        // 0xF14 - Hardware thread ID
    pub mvendorid: u64,      // 0xF11 - Vendor ID
    pub marchid: u64,        // 0xF12 - Architecture ID
    pub mimpid: u64,         // 0xF13 - Implementation ID

    // Machine Trap Setup
    pub mstatus: u64,        // 0x300 - Machine status
    pub misa: u64,           // 0x301 - ISA and extensions
    pub medeleg: u64,        // 0x302 - Machine exception delegation
    pub mideleg: u64,        // 0x303 - Machine interrupt delegation
    pub mie: u64,            // 0x304 - Machine interrupt enable
    pub mtvec: u64,          // 0x305 - Machine trap-handler base address
    pub mcounteren: u64,     // 0x306 - Machine counter enable

    // Machine Trap Handling
    pub mscratch: u64,       // 0x340 - Scratch register for machine trap handlers
    pub mepc: u64,           // 0x341 - Machine exception program counter
    pub mcause: u64,         // 0x342 - Machine trap cause
    pub mtval: u64,          // 0x343 - Machine bad address or instruction
    pub mip: u64,            // 0x344 - Machine interrupt pending

    // Machine Counter/Timers (read-only shadows)
    pub mcycle: u64,         // 0xB00 - Machine cycle counter
    pub minstret: u64,       // 0xB02 - Machine instructions-retired counter

    // Supervisor Trap Setup
    pub stvec: u64,          // 0x105 - Supervisor trap handler base address
    pub scounteren: u64,     // 0x106 - Supervisor counter enable

    // Supervisor Trap Handling
    pub sscratch: u64,       // 0x140 - Scratch register for supervisor trap handlers
    pub sepc: u64,           // 0x141 - Supervisor exception program counter
    pub scause: u64,         // 0x142 - Supervisor trap cause
    pub stval: u64,          // 0x143 - Supervisor bad address or instruction
    pub sip: u64,            // 0x144 - Supervisor interrupt pending

    // Supervisor Protection and Translation
    pub satp: u64,           // 0x180 - Supervisor address translation and protection

    // User Trap Handling
    pub ustatus: u64,        // 0x000 - User status (shadow of mstatus/sstatus)
    pub uepc: u64,           // 0x041 - User exception program counter
    pub ucause: u64,         // 0x042 - User trap cause
    pub utval: u64,          // 0x043 - User bad address or instruction

    // Vector extension CSRs
    pub vl: u64,             // 0xC20 - Vector length
    pub vtype: u64,          // 0xC21 - Vector type
    pub vlenb: u64,          // 0xC22 - Vector register length in bytes
    pub vstart: u64,         // 0x008 - Vector start position
    pub vxsat: u64,          // 0x009 - Fixed-point saturation flag
    pub vxrm: u64,           // 0x00A - Fixed-point rounding mode
    pub vcsr: u64,           // 0x00F - Vector control and status register
}

/// Supervisor status register (SSTATUS) is a restricted view of MSTATUS.
/// SSTATUS mask: only SIE, SPIE, SPP, FS, XS, SD, UXL bits are accessible in S-mode.
const SSTATUS_MASK: u64 = 0x8000_0003_000C_6622;

/// Supervisor interrupt enable register (SIE) is a restricted view of MIE.
/// SIE mask: only SEIE, STIE, SSIE bits.
const SIE_MASK: u64 = 0x0000_0000_0000_0222;

impl CsrFile {
    /// Read sstatus (restricted view of mstatus)
    pub fn sstatus(&self) -> u64 {
        self.mstatus & SSTATUS_MASK
    }

    /// Write sstatus (only writable bits from SSTATUS_MASK are updated in mstatus)
    pub fn set_sstatus(&mut self, value: u64) {
        self.mstatus = (self.mstatus & !SSTATUS_MASK) | (value & SSTATUS_MASK);
    }

    /// Read sie (restricted view of mie)
    pub fn sie(&self) -> u64 {
        self.mie & SIE_MASK
    }

    /// Write sie (only writable bits from SIE_MASK are updated in mie)
    pub fn set_sie(&mut self, value: u64) {
        self.mie = (self.mie & !SIE_MASK) | (value & SIE_MASK);
    }
}

impl Default for CsrFile {
    fn default() -> Self {
        Self {
            mhartid: 0,
            mvendorid: 0,
            marchid: 0,
            mimpid: 0,
            mstatus: 0,
            misa: 0,
            medeleg: 0,
            mideleg: 0,
            mie: 0,
            mtvec: 0,
            mcounteren: 0,
            mscratch: 0,
            mepc: 0,
            mcause: 0,
            mtval: 0,
            mip: 0,
            mcycle: 0,
            minstret: 0,
            stvec: 0,
            scounteren: 0,
            sscratch: 0,
            sepc: 0,
            scause: 0,
            stval: 0,
            sip: 0,
            satp: 0,
            ustatus: 0,
            uepc: 0,
            ucause: 0,
            utval: 0,
            vl: 0,
            vtype: 0,
            vlenb: 0,
            vstart: 0,
            vxsat: 0,
            vxrm: 0,
            vcsr: 0,
        }
    }
}

/// Privilege levels for RISC-V
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivilegeLevel {
    User = 0,
    Supervisor = 1,
    Machine = 3,
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
    /// Instructions per cycle based on actual execution cycles.
    pub fn ipc(&self) -> f64 {
        if self.exec_cycles > 0 {
            self.inst_count as f64 / self.exec_cycles as f64
        } else {
            0.0
        }
    }
}

/// Exception cause values (mcause/scause)
pub const EXCEPTION_INSTRUCTION_ADDRESS_MISALIGNED: u64 = 0;
pub const EXCEPTION_INSTRUCTION_ACCESS_FAULT: u64 = 1;
pub const EXCEPTION_ILLEGAL_INSTRUCTION: u64 = 2;
pub const EXCEPTION_BREAKPOINT: u64 = 3;
pub const EXCEPTION_LOAD_ADDRESS_MISALIGNED: u64 = 4;
pub const EXCEPTION_LOAD_ACCESS_FAULT: u64 = 5;
pub const EXCEPTION_STORE_ADDRESS_MISALIGNED: u64 = 6;
pub const EXCEPTION_STORE_ACCESS_FAULT: u64 = 7;
pub const EXCEPTION_ENVIRONMENT_CALL_FROM_U: u64 = 8;
pub const EXCEPTION_ENVIRONMENT_CALL_FROM_S: u64 = 9;
pub const EXCEPTION_ENVIRONMENT_CALL_FROM_M: u64 = 11;

/// MSTATUS bit definitions
pub const MSTATUS_SIE: u64 = 1 << 1;   // Supervisor interrupt enable
pub const MSTATUS_MIE: u64 = 1 << 3;   // Machine interrupt enable
pub const MSTATUS_SPIE: u64 = 1 << 5;  // Supervisor previous interrupt enable
pub const MSTATUS_MPIE: u64 = 1 << 7;  // Machine previous interrupt enable
pub const MSTATUS_SPP: u64 = 1 << 8;   // Supervisor previous privilege
pub const MSTATUS_MPP_MASK: u64 = 0x3 << 11; // Machine previous privilege mask
pub const MSTATUS_MPP_M: u64 = 3 << 11;      // Machine previous privilege = Machine

pub const PC_INCREMENT: usize = 4;

/// Vector execution state (VL, SEW, LMUL)
#[derive(Clone, Debug, Default)]
pub struct VectorState {
    pub vl: usize,
    pub sew: usize,
    pub lmul: usize,
}

#[derive(Clone, Debug)]
pub struct Hart {
    pub id: u64,
    pub pc: usize,
    pub program_id: ProgramId,
    pub x: IntegerRegisterFile,
    pub f: FloatRegisterFile,
    pub v: VectorRegisterFile,
    pub vector_state: VectorState,
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
    pub reservation: Option<u64>,
    /// Current privilege level (defaults to Machine mode)
    pub privilege: PrivilegeLevel,
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
            vector_state: VectorState::default(),
            csr: CsrFile::default(),
            reservation: None,
            privilege: PrivilegeLevel::Machine,
            elapsed_cycles: 0,
            exec_cycles: 0,
            inst_count: 0,
            start_clock: 0,
            state: HartState::Running,
            finish_clock: None,
            error_info: None,
        }
    }

    /// Take a trap: save context and jump to trap handler.
    /// Returns the new PC.
    pub fn take_trap(&mut self, cause: u64, tval: u64, is_interrupt: bool) {
        let cause_bit = if is_interrupt { 1u64 << 63 } else { 0 };
        let delegated = self.is_trap_delegated(cause, is_interrupt);

        if delegated == PrivilegeLevel::Supervisor {
            // Delegate to S-mode
            let priv_val = self.privilege as u64;
            self.csr.scause = cause_bit | cause;
            self.csr.stval = tval;
            self.csr.sepc = self.pc as u64;
            // Save S-mode state
            let sie = (self.csr.mstatus & MSTATUS_SIE) != 0;
            self.csr.mstatus = self.csr.mstatus & !MSTATUS_SIE;
            if sie {
                self.csr.mstatus |= MSTATUS_SPIE;
            }
            self.csr.mstatus = (self.csr.mstatus & !MSTATUS_SPP) | ((priv_val & 1) << 8);
            self.privilege = PrivilegeLevel::Supervisor;
            // Jump to supervisor trap handler
            self.pc = self.csr.stvec as usize;
        } else {
            // Delegate to M-mode (default)
            let priv_val = self.privilege as u64;
            self.csr.mcause = cause_bit | cause;
            self.csr.mtval = tval;
            self.csr.mepc = self.pc as u64;
            // Save M-mode state
            let mie = (self.csr.mstatus & MSTATUS_MIE) != 0;
            self.csr.mstatus = self.csr.mstatus & !MSTATUS_MIE;
            if mie {
                self.csr.mstatus |= MSTATUS_MPIE;
            }
            self.csr.mstatus = (self.csr.mstatus & !MSTATUS_MPP_MASK) | (priv_val << 11);
            self.privilege = PrivilegeLevel::Machine;
            // Jump to machine trap handler
            self.pc = self.csr.mtvec as usize;
        }
    }

    /// Return from trap (MRET/SRET/URET)
    pub fn return_from_trap(&mut self, target_priv: PrivilegeLevel) {
        match target_priv {
            PrivilegeLevel::Machine => {
                // MRET: restore from mstatus
                let mpp = (self.csr.mstatus & MSTATUS_MPP_MASK) >> 11;
                self.privilege = match mpp {
                    0 => PrivilegeLevel::User,
                    1 => PrivilegeLevel::Supervisor,
                    _ => PrivilegeLevel::Machine,
                };
                let mpie = (self.csr.mstatus & MSTATUS_MPIE) != 0;
                self.csr.mstatus = self.csr.mstatus & !MSTATUS_MPIE;
                self.csr.mstatus |= if mpie { MSTATUS_MIE } else { 0 };
                self.pc = self.csr.mepc as usize;
            }
            PrivilegeLevel::Supervisor => {
                // SRET: restore from sstatus
                let spp = (self.csr.mstatus & MSTATUS_SPP) >> 8;
                self.privilege = match spp {
                    0 => PrivilegeLevel::User,
                    _ => PrivilegeLevel::Supervisor,
                };
                let spie = (self.csr.mstatus & MSTATUS_SPIE) != 0;
                self.csr.mstatus = self.csr.mstatus & !MSTATUS_SPIE;
                self.csr.mstatus |= if spie { MSTATUS_SIE } else { 0 };
                self.csr.mstatus = self.csr.mstatus & !MSTATUS_SPP;
                self.pc = self.csr.sepc as usize;
            }
            PrivilegeLevel::User => {
                // URET: restore from ustatus
                self.privilege = PrivilegeLevel::User;
                self.pc = self.csr.uepc as usize;
            }
        }
    }

    /// Check if a trap should be delegated to supervisor mode.
    /// RISC-V privileged spec: traps from M-mode are always handled in M-mode.
    /// Traps from S/U-mode check medeleg/mideleg; if the corresponding bit is
    /// set the trap is delegated to S-mode, otherwise it stays in M-mode.
    fn is_trap_delegated(&self, cause: u64, is_interrupt: bool) -> PrivilegeLevel {
        if self.privilege == PrivilegeLevel::Machine {
            return PrivilegeLevel::Machine; // M-mode traps always handled in M-mode
        }
        if is_interrupt {
            if (self.csr.mideleg & (1 << cause)) != 0 {
                PrivilegeLevel::Supervisor
            } else {
                PrivilegeLevel::Machine
            }
        } else {
            if (self.csr.medeleg & (1 << cause)) != 0 {
                PrivilegeLevel::Supervisor
            } else {
                PrivilegeLevel::Machine
            }
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
                // P0-7/P0-11: Detect RISC-V NaN-boxing single-precision
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
                    // per RISC-V privileged spec 3.1.1. Allowing writes would
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
                    _ => {
                        // reg.index holds the CSR address
                        self.write_csr(reg.index as u64, v);
                    }
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

    /// Read a CSR by its address
    pub fn read_csr(&self, addr: u64) -> u64 {
        match addr {
            // User CSRs
            0x000 => self.csr.ustatus,
            0x041 => self.csr.uepc,
            0x042 => self.csr.ucause,
            0x043 => self.csr.utval,
            // Vector CSRs
            0x008 => self.csr.vstart,
            0x009 => self.csr.vxsat,
            0x00A => self.csr.vxrm,
            0x00F => self.csr.vcsr,
            // Supervisor CSRs
            0x100 => self.csr.sstatus(),
            0x104 => self.csr.sie(),
            0x105 => self.csr.stvec,
            0x106 => self.csr.scounteren,
            0x140 => self.csr.sscratch,
            0x141 => self.csr.sepc,
            0x142 => self.csr.scause,
            0x143 => self.csr.stval,
            0x144 => self.csr.sip,
            0x180 => self.csr.satp,
            // Machine CSRs
            0x300 => self.csr.mstatus,
            0x301 => self.csr.misa,
            0x302 => self.csr.medeleg,
            0x303 => self.csr.mideleg,
            0x304 => self.csr.mie,
            0x305 => self.csr.mtvec,
            0x306 => self.csr.mcounteren,
            0x340 => self.csr.mscratch,
            0x341 => self.csr.mepc,
            0x342 => self.csr.mcause,
            0x343 => self.csr.mtval,
            0x344 => self.csr.mip,
            0xB00 => self.csr.mcycle,
            0xB02 => self.csr.minstret,
            0xF11 => self.csr.mvendorid,
            0xF12 => self.csr.marchid,
            0xF13 => self.csr.mimpid,
            0xF14 => self.csr.mhartid,
            // Vector CSRs
            0xC20 => self.csr.vl,
            0xC21 => self.csr.vtype,
            0xC22 => self.csr.vlenb,
            _ => 0,
        }
    }

    /// Write a value to a CSR by its address
    pub fn write_csr(&mut self, addr: u64, value: u64) {
        match addr {
            // User CSRs
            0x000 => self.csr.ustatus = value,
            0x041 => self.csr.uepc = value,
            0x042 => self.csr.ucause = value,
            0x043 => self.csr.utval = value,
            // Vector CSRs
            0x008 => self.csr.vstart = value,
            0x009 => self.csr.vxsat = value,
            0x00A => self.csr.vxrm = value,
            0x00F => self.csr.vcsr = value,
            // Supervisor CSRs
            0x100 => self.csr.set_sstatus(value),
            0x104 => self.csr.set_sie(value),
            0x105 => self.csr.stvec = value,
            0x106 => self.csr.scounteren = value,
            0x140 => self.csr.sscratch = value,
            0x141 => self.csr.sepc = value,
            0x142 => self.csr.scause = value,
            0x143 => self.csr.stval = value,
            0x144 => self.csr.sip = value,
            0x180 => self.csr.satp = value,
            // Machine CSRs
            0x300 => self.csr.mstatus = value,
            0x301 => self.csr.misa = value,
            0x302 => self.csr.medeleg = value,
            0x303 => self.csr.mideleg = value,
            0x304 => self.csr.mie = value,
            0x305 => self.csr.mtvec = value,
            0x306 => self.csr.mcounteren = value,
            0x340 => self.csr.mscratch = value,
            0x341 => self.csr.mepc = value,
            0x342 => self.csr.mcause = value,
            0x343 => self.csr.mtval = value,
            0x344 => self.csr.mip = value,
            0xB00 => self.csr.mcycle = value,
            0xB02 => self.csr.minstret = value,
            0xF11 => self.csr.mvendorid = value,
            0xF12 => self.csr.marchid = value,
            0xF13 => self.csr.mimpid = value,
            0xF14 => self.csr.mhartid = value,
            // Vector CSRs
            0xC20 => self.csr.vl = value,
            0xC21 => self.csr.vtype = value,
            0xC22 => self.csr.vlenb = value,
            _ => {}
        }
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
            vector_state: VectorState::default(),
            csr: CsrFile::default(),
            reservation: None,
            privilege: PrivilegeLevel::Machine,
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