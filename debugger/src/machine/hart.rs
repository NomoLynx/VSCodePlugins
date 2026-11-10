//! Runtime state of a single RISC-V hardware thread (hart).
//!
//! This module models the architectural state of a hart: its register files
//! (integer, float, vector), its CSR file, privilege level, and the trap
//! entry/return paths.

use crate::debugger_error::DebuggerError;
use crate::machine::{
    machine::ProgramId,
    register_ref::{RegisterRef, RegisterType},
    runtime_value::RuntimeValue,
};

/// A single register slot: its value plus optional provenance metadata.
#[derive(Clone, Debug)]
pub(crate) struct RegValue<T> {
    /// The stored value.
    pub(crate) value: T,
    /// Optional provenance (e.g. value origin) used for data-flow tracking.
    pub(crate) provenance: Option<u64>,
}

/// Integer register file (`x0`-`x31`). Reads/writes to `x0` are hardwired to 0.
#[derive(Clone, Debug)]
pub struct IntegerRegisterFile {
    /// The backing register slots, indexed by register number.
    pub(crate) regs: Vec<RegValue<u64>>,
}

impl IntegerRegisterFile {
    /// Returns the value of `reg`, or 0 if `reg` is 0 or out of range.
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

    /// Writes `value` to `reg`. Writes to `x0` are silently dropped.
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

    /// Creates a register file with `size` zeroed slots.
    #[must_use]
    pub fn new(size: usize) -> Self {
        Self {
            regs: vec![RegValue { value: 0, provenance: None }; size],
        }
    }
}

impl Default for IntegerRegisterFile {
    /// Creates a 32-entry integer register file (`x0`-`x31`).
    fn default() -> Self {
        Self::new(32)
    }
}

/// Float register file stores raw 64-bit patterns.
/// All single/double-precision ops interpret bits on-the-fly.
/// Single-precision writes NaN-box bits[63:32]=0xFFFFFFFF per RISC-V spec.
#[derive(Clone, Debug)]
pub struct FloatRegisterFile {
    /// The backing register slots, indexed by register number.
    pub(crate) regs: Vec<RegValue<u64>>,
}

impl FloatRegisterFile {
    /// Returns the raw 64-bit pattern in `reg`, or 0 if out of range.
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

    /// Writes a raw 64-bit pattern to `reg`. Out-of-range writes are dropped.
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

    /// Creates a register file with `size` zeroed slots.
    #[must_use]
    pub fn new(size: usize) -> Self {
        Self {
            regs: vec![RegValue { value: 0, provenance: None }; size],
        }
    }
}

impl Default for FloatRegisterFile {
    /// Creates a 32-entry float register file (`f0`-`f31`).
    fn default() -> Self {
        Self::new(32)
    }
}

/// A single vector register holding a fixed-width byte pattern.
#[derive(Clone, Debug)]
pub struct VectorRegister {
    /// Raw byte storage for the register (512 bits = 64 bytes).
    pub(crate) bytes: Vec<u8>,
}

impl Default for VectorRegister {
    /// Creates a zeroed 64-byte (512-bit) vector register.
    fn default() -> Self {
        Self {
            bytes: vec![0; 64],
        }
    }
}

/// Vector register file (`v0`-`v31`).
#[derive(Clone, Debug)]
pub struct VectorRegisterFile {
    /// The backing vector registers, indexed by register number.
    pub(crate) regs: Vec<VectorRegister>,
}

impl VectorRegisterFile {
    /// Returns a copy of the byte pattern in `reg`, or `None` if out of range.
    #[must_use]
    pub fn read_bytes(&self, reg: usize) -> Option<Vec<u8>> {
        self.regs.get(reg).map(|vr| vr.bytes.clone())
    }

    /// Copies `data` into `reg`, truncating to the register width.
    ///
    /// Returns `false` if `reg` is out of range.
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
    /// Creates a 32-entry zeroed vector register file.
    fn default() -> Self {
        Self {
            regs: vec![VectorRegister::default(); 32],
        }
    }
}

/// RISC-V control and status registers (CSRs) for a single hart.
///
/// Fields are grouped by CSR category (machine, supervisor, user, vector).
#[derive(Clone, Debug)]
pub struct CsrFile {
    // Machine Information Registers
    /// Hardware thread ID (0xF14).
    pub(crate) mhartid: u64,
    /// Vendor ID (0xF11).
    pub(crate) mvendorid: u64,
    /// Architecture ID (0xF12).
    pub(crate) marchid: u64,
    /// Implementation ID (0xF13).
    pub(crate) mimpid: u64,

    // Machine Trap Setup
    /// Machine status (0x300).
    pub(crate) mstatus: u64,
    /// ISA and extensions (0x301).
    pub(crate) misa: u64,
    /// Machine exception delegation (0x302).
    pub(crate) medeleg: u64,
    /// Machine interrupt delegation (0x303).
    pub(crate) mideleg: u64,
    /// Machine interrupt enable (0x304).
    pub(crate) mie: u64,
    /// Machine trap-handler base address (0x305).
    pub(crate) mtvec: u64,
    /// Machine counter enable (0x306).
    pub(crate) mcounteren: u64,

    // Machine Trap Handling
    /// Scratch register for machine trap handlers (0x340).
    pub(crate) mscratch: u64,
    /// Machine exception program counter (0x341).
    pub(crate) mepc: u64,
    /// Machine trap cause (0x342).
    pub(crate) mcause: u64,
    /// Machine bad address or instruction (0x343).
    pub(crate) mtval: u64,
    /// Machine interrupt pending (0x344).
    pub(crate) mip: u64,

    // Machine Counter/Timers (read-only shadows)
    /// Machine cycle counter (0xB00).
    pub(crate) mcycle: u64,
    /// Machine instructions-retired counter (0xB02).
    pub(crate) minstret: u64,

    // Supervisor Trap Setup
    /// Supervisor trap handler base address (0x105).
    pub(crate) stvec: u64,
    /// Supervisor counter enable (0x106).
    pub(crate) scounteren: u64,

    // Supervisor Trap Handling
    /// Scratch register for supervisor trap handlers (0x140).
    pub(crate) sscratch: u64,
    /// Supervisor exception program counter (0x141).
    pub(crate) sepc: u64,
    /// Supervisor trap cause (0x142).
    pub(crate) scause: u64,
    /// Supervisor bad address or instruction (0x143).
    pub(crate) stval: u64,
    /// Supervisor interrupt pending (0x144).
    pub(crate) sip: u64,

    // Supervisor Protection and Translation
    /// Supervisor address translation and protection (0x180).
    pub(crate) satp: u64,

    // User Trap Handling
    /// User status, shadow of mstatus/sstatus (0x000).
    pub(crate) ustatus: u64,
    /// User exception program counter (0x041).
    pub(crate) uepc: u64,
    /// User trap cause (0x042).
    pub(crate) ucause: u64,
    /// User bad address or instruction (0x043).
    pub(crate) utval: u64,

    // Vector extension CSRs
    /// Vector length (0xC20).
    pub(crate) vl: u64,
    /// Vector type (0xC21).
    pub(crate) vtype: u64,
    /// Vector register length in bytes (0xC22).
    pub(crate) vlenb: u64,
    /// Vector start position (0x008).
    pub(crate) vstart: u64,
    /// Fixed-point saturation flag (0x009).
    pub(crate) vxsat: u64,
    /// Fixed-point rounding mode (0x00A).
    pub(crate) vxrm: u64,
    /// Vector control and status register (0x00F).
    pub(crate) vcsr: u64,
}

/// Supervisor status register (SSTATUS) is a restricted view of MSTATUS.
/// SSTATUS mask: only SIE, SPIE, SPP, FS, XS, SD, UXL bits are accessible in S-mode.
const SSTATUS_MASK: u64 = 0x8000_0003_000C_6622;

/// Supervisor interrupt enable register (SIE) is a restricted view of MIE.
/// SIE mask: only SEIE, STIE, SSIE bits.
const SIE_MASK: u64 = 0x0000_0000_0000_0222;

impl CsrFile {
    /// Reads `sstatus` (restricted view of `mstatus`).
    #[must_use]
    pub fn sstatus(&self) -> u64 {
        self.mstatus & SSTATUS_MASK
    }

    /// Writes `sstatus` (only writable bits from `SSTATUS_MASK` are updated in `mstatus`).
    pub fn set_sstatus(&mut self, value: u64) {
        self.mstatus = (self.mstatus & !SSTATUS_MASK) | (value & SSTATUS_MASK);
    }

    /// Reads `sie` (restricted view of `mie`).
    #[must_use]
    pub fn sie(&self) -> u64 {
        self.mie & SIE_MASK
    }

    /// Writes `sie` (only writable bits from `SIE_MASK` are updated in `mie`).
    pub fn set_sie(&mut self, value: u64) {
        self.mie = (self.mie & !SIE_MASK) | (value & SIE_MASK);
    }
}

impl Default for CsrFile {
    /// Creates a CSR file with all registers zeroed.
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

/// Privilege levels for RISC-V.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivilegeLevel {
    /// User mode (privilege 0).
    User = 0,
    /// Supervisor mode (privilege 1).
    Supervisor = 1,
    /// Machine mode (privilege 3).
    Machine = 3,
}

/// RISC-V standard CSR addresses used for register index mapping.
pub const CSR_MHARTID: usize = 0xF14;
/// CSR address of the `vl` register.
pub const CSR_VL: usize = 0xC20;
/// CSR address of the `vtype` register.
pub const CSR_VTYPE: usize = 0xC21;

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
    /// Hardware thread ID this snapshot belongs to.
    pub(crate) hart_id: u64,
    /// Index of the processor hosting this hart.
    pub(crate) processor_id: usize,
    /// Current runtime state of the hart.
    pub(crate) state: HartState,
    /// Clock cycles this hart has taken part in (stop-the-world model: frozen
    /// while another hart is being single-stepped, and after this hart is
    /// Finished).
    pub(crate) elapsed_cycles: u64,
    /// Total execution cycles consumed by instructions executed on this hart.
    /// Used for IPC calculation; grows only when an instruction actually executes.
    pub(crate) exec_cycles: u64,
    /// Total instructions executed by this hart.
    pub(crate) inst_count: u64,
    /// Runtime cycles since this hart was created (`elapsed_cycles - start_clock`).
    /// Frozen while this hart is not taking part (stop-the-world stepping) and
    /// once it reaches `Finished`.
    pub(crate) runtime_cycles: u64,
}

impl HartPerformance {
    /// Instructions per cycle based on actual execution cycles.
    #[must_use]
    pub fn ipc(&self) -> f64 {
        if self.exec_cycles > 0 {
            self.inst_count as f64 / self.exec_cycles as f64
        } else {
            0.0
        }
    }
}

/// Exception cause values (mcause/scause).
pub const EXCEPTION_INSTRUCTION_ADDRESS_MISALIGNED: u64 = 0;
/// Instruction access fault.
pub const EXCEPTION_INSTRUCTION_ACCESS_FAULT: u64 = 1;
/// Illegal instruction.
pub const EXCEPTION_ILLEGAL_INSTRUCTION: u64 = 2;
/// Breakpoint.
pub const EXCEPTION_BREAKPOINT: u64 = 3;
/// Load address misaligned.
pub const EXCEPTION_LOAD_ADDRESS_MISALIGNED: u64 = 4;
/// Load access fault.
pub const EXCEPTION_LOAD_ACCESS_FAULT: u64 = 5;
/// Store/AMO address misaligned.
pub const EXCEPTION_STORE_ADDRESS_MISALIGNED: u64 = 6;
/// Store/AMO access fault.
pub const EXCEPTION_STORE_ACCESS_FAULT: u64 = 7;
/// Environment call from U-mode.
pub const EXCEPTION_ENVIRONMENT_CALL_FROM_U: u64 = 8;
/// Environment call from S-mode.
pub const EXCEPTION_ENVIRONMENT_CALL_FROM_S: u64 = 9;
/// Environment call from M-mode.
pub const EXCEPTION_ENVIRONMENT_CALL_FROM_M: u64 = 11;

/// MSTATUS bit definitions.
pub const MSTATUS_SIE: u64 = 1 << 1; // Supervisor interrupt enable
/// Machine interrupt enable.
pub const MSTATUS_MIE: u64 = 1 << 3;
/// Supervisor previous interrupt enable.
pub const MSTATUS_SPIE: u64 = 1 << 5;
/// Machine previous interrupt enable.
pub const MSTATUS_MPIE: u64 = 1 << 7;
/// Supervisor previous privilege.
pub const MSTATUS_SPP: u64 = 1 << 8;
/// Machine previous privilege mask.
pub const MSTATUS_MPP_MASK: u64 = 0x3 << 11;
/// Machine previous privilege = Machine.
pub const MSTATUS_MPP_M: u64 = 3 << 11;

/// Default byte size of a standard (non-compressed) instruction.
pub const PC_INCREMENT: usize = 4;

/// Vector execution state (VL, SEW, LMUL).
#[derive(Clone, Debug, Default)]
pub struct VectorState {
    /// Vector length (active elements).
    pub(crate) vl: usize,
    /// Selected element width in bits.
    pub(crate) sew: usize,
    /// Length multiplier.
    pub(crate) lmul: usize,
}

/// Runtime state of a single RISC-V hardware thread.
#[derive(Clone, Debug)]
pub struct Hart {
    /// Hardware thread ID.
    pub(crate) id: u64,
    /// Program counter (byte address of the next instruction).
    pub(crate) pc: usize,
    /// Identifier of the program loaded on this hart.
    pub(crate) program_id: ProgramId,
    /// Integer register file (`x0`-`x31`).
    pub(crate) x: IntegerRegisterFile,
    /// Float register file (`f0`-`f31`).
    pub(crate) f: FloatRegisterFile,
    /// Vector register file (`v0`-`v31`).
    pub(crate) v: VectorRegisterFile,
    /// Vector execution state (VL, SEW, LMUL).
    pub(crate) vector_state: VectorState,
    /// Control and status registers.
    pub(crate) csr: CsrFile,
    /// Clock cycles this hart has taken part in.  Stop-the-world model: this
    /// advances only while this hart actually executes (or takes part in a
    /// `step_all_harts` round), so a hart frozen during another hart's
    /// single-step does not accumulate.  Kept aligned with `start_clock` at
    /// creation/re-init so `elapsed_cycles - start_clock` is the runtime.
    pub(crate) elapsed_cycles: u64,
    /// Total execution cycles consumed by instructions executed on this hart.
    /// Used for IPC calculation; grows only when an instruction actually executes.
    pub(crate) exec_cycles: u64,
    /// Number of instructions executed by this hart.
    pub(crate) inst_count: u64,
    /// Global clock value at the moment this hart was created / re-initialized.
    pub(crate) start_clock: u64,
    /// Current runtime state of this hart.
    pub(crate) state: HartState,
    /// Processor clock value at the moment this hart transitioned to Finished.
    /// Kept as metadata (when execution ended).  Runtime freezing is now
    /// inherent to the stop-the-world model: a Finished hart no longer executes,
    /// so `elapsed_cycles` — and hence `elapsed_cycles - start_clock` — stops
    /// growing on its own.
    pub(crate) finish_clock: Option<u64>,
    /// If the hart terminated due to an execution error (illegal instruction,
    /// memory access fault, etc.), this holds the error description.
    /// `None` means normal termination (PC past end of text) or still running.
    pub(crate) error_info: Option<String>,
    /// Atomic reservation address (LR/SC), if any.
    pub(crate) reservation: Option<u64>,
    /// Current privilege level (defaults to Machine mode).
    pub(crate) privilege: PrivilegeLevel,
}

impl Hart {
    /// Creates a new running hart with the given `id` and `program_id`.
    #[must_use]
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

    /// Takes a trap: saves context and jumps to the appropriate trap handler.
    ///
    /// Delegates to S-mode or M-mode based on [`Hart::is_trap_delegated`].
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
            self.csr.mstatus &= !MSTATUS_SIE;
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
            self.csr.mstatus &= !MSTATUS_MIE;
            if mie {
                self.csr.mstatus |= MSTATUS_MPIE;
            }
            self.csr.mstatus = (self.csr.mstatus & !MSTATUS_MPP_MASK) | (priv_val << 11);
            self.privilege = PrivilegeLevel::Machine;
            // Jump to machine trap handler
            self.pc = self.csr.mtvec as usize;
        }
    }

    /// Returns from a trap (MRET/SRET/URET), restoring privilege and PC.
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
                self.csr.mstatus &= !MSTATUS_MPIE;
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
                self.csr.mstatus &= !MSTATUS_SPIE;
                self.csr.mstatus |= if spie { MSTATUS_SIE } else { 0 };
                self.csr.mstatus &= !MSTATUS_SPP;
                self.pc = self.csr.sepc as usize;
            }
            PrivilegeLevel::User => {
                // URET: restore from ustatus
                self.privilege = PrivilegeLevel::User;
                self.pc = self.csr.uepc as usize;
            }
        }
    }

    /// Checks if a trap should be delegated to supervisor mode.
    ///
    /// RISC-V privileged spec: traps from M-mode are always handled in M-mode.
    /// Traps from S/U-mode check `medeleg`/`mideleg`; if the corresponding bit
    /// is set the trap is delegated to S-mode, otherwise it stays in M-mode.
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
        } else if (self.csr.medeleg & (1 << cause)) != 0 {
            PrivilegeLevel::Supervisor
        } else {
            PrivilegeLevel::Machine
        }
    }

    /// Increments the program counter by [`PC_INCREMENT`].
    pub fn next_pc(&mut self) {
        self.pc += PC_INCREMENT;
    }

    /// Sets the program counter to `pc`.
    pub fn set_pc(&mut self, pc: usize) {
        self.pc = pc;
    }

    /// Reads the register addressed by `reg`.
    pub fn read_register(&self, reg: &RegisterRef) -> RuntimeValue {
        match reg.reg_type {
            RegisterType::Integer => RuntimeValue::Integer(self.x.read(reg.index)),
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
                RuntimeValue::Vector(self.v.read_bytes(reg.index).unwrap_or_default())
            }
            RegisterType::Csr => match reg.index {
                CSR_MHARTID => RuntimeValue::Integer(self.csr.mhartid),
                CSR_VL => RuntimeValue::Integer(self.csr.vl),
                CSR_VTYPE => RuntimeValue::Integer(self.csr.vtype),
                _ => RuntimeValue::Unavailable,
            },
        }
    }

    /// Writes `value` to the register addressed by `reg`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::RegisterWriteError`] on a vector-register range
    /// error, a write to a read-only CSR, or a type mismatch between the
    /// register kind and the value supplied.
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
                    return Err(DebuggerError::RegisterWriteError(format!(
                        "vector register index {} out of range (max 31)",
                        reg.index
                    )));
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
                            other => return Err(DebuggerError::RegisterWriteError(format!(
                                "unexpected read-only CSR index {other}"
                            ))),
                        };
                        return Err(DebuggerError::RegisterWriteError(format!(
                            "{} is a read-only CSR per RISC-V spec",
                            name
                        )));
                    }
                    _ => {
                        // reg.index holds the CSR address
                        self.write_csr(reg.index as u64, v);
                    }
                }
            }

            (unexpected_type, unexpected_value) => {
                return Err(DebuggerError::RegisterWriteError(format!(
                    "register type mismatch: expected {:?}, got {:?}",
                    unexpected_type, unexpected_value
                )));
            }
        }

        Ok(())
    }

    /// Reads the CSR at `addr`, or 0 for an unmapped address.
    #[must_use]
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

    /// Writes `value` to the CSR at `addr`. Unmapped addresses are ignored.
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
    /// Creates a running hart at id 0 / program 0.
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
