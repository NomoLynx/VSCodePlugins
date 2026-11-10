//! Crate-wide error type and result alias.

use thiserror::Error;

/// Convenience alias for fallible operations across the debugger.
pub type AppResult<T> = Result<T, DebuggerError>;

/// Errors produced by any layer of the debugger (memory, registers, execution,
/// harts, program loading, DAP).
#[derive(Debug, Clone, PartialEq, Error)]
pub enum DebuggerError {
    /// A memory access failed (e.g. access to an unmapped address).
    #[error("Memory access error at address 0x{addr:x}, size {size}")]
    MemoryAccessError { addr: u64, size: usize },

    /// A memory access fell outside all mapped regions.
    #[error("Memory out of range: address 0x{addr:x}, size {size}")]
    MemoryOutOfRange { addr: u64, size: usize },

    /// An invalid register was referenced.
    #[error("Invalid {reg_type} register at index {index}")]
    InvalidRegister { reg_type: String, index: usize },

    /// Failed to read a register.
    #[error("Register read error: {0}")]
    RegisterReadError(String),

    /// Failed to write a register.
    #[error("Register write error: {0}")]
    RegisterWriteError(String),

    /// The instruction at `pc` could not be decoded.
    #[error("Invalid instruction at PC 0x{pc:x}")]
    InvalidInstruction { pc: usize },

    /// The decoded opcode is not implemented by the simulator.
    #[error("Unsupported instruction: {opcode}")]
    UnsupportedInstruction { opcode: String },

    /// A generic execution failure.
    #[error("Execution error: {0}")]
    ExecutionError(String),

    /// The provided hart ID is invalid.
    #[error("Invalid hart ID: {hart_id}")]
    InvalidHartId { hart_id: u64 },

    /// The provided hart ID does not exist.
    #[error("Hart not found: {hart_id}")]
    HartNotFound { hart_id: u64 },

    /// Failed to load a program.
    #[error("Program load error: {0}")]
    ProgramLoadError(String),

    /// The program has no discoverable entry point.
    #[error("Entry point not found for program {program_id}")]
    EntryPointNotFound { program_id: usize },

    /// A DAP protocol error.
    #[error("DAP error: {0}")]
    DapError(String),

    /// A fallback error carrying a free-form message.
    #[error("{0}")]
    GeneralError(String),
}
