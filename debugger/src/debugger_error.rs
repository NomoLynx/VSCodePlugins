#[derive(Debug, Clone, PartialEq)]
pub enum DebuggerError {
    // Memory related error
    MemoryAccessError { addr: u64, size: usize },
    MemoryOutOfRange { addr: u64, size: usize },
    
    // Register related error
    InvalidRegister { reg_type: String, index: usize },
    RegisterReadError(String),
    RegisterWriteError(String),
    
    // Execution related error
    InvalidInstruction { pc: usize },
    UnsupportedInstruction { opcode: String },
    ExecutionError(String),
    
    // Hart related error
    InvalidHartId { hart_id: u64 },
    HartNotFound { hart_id: u64 },
    
    // Load program error
    ProgramLoadError(String),
    EntryPointNotFound { program_id: usize },
    
    // DAP related error
    DapError(String),

    // Common error
    GeneralError(String),
}

impl std::error::Error for DebuggerError {}

impl std::fmt::Display for DebuggerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MemoryAccessError { addr, size } => 
                write!(f, "Memory access error at address 0x{:x}, size {}", addr, size),
            Self::MemoryOutOfRange { addr, size } => 
                write!(f, "Memory out of range: address 0x{:x}, size {}", addr, size),
            Self::InvalidRegister { reg_type, index } => 
                write!(f, "Invalid {} register at index {}", reg_type, index),
            Self::RegisterReadError(msg) => 
                write!(f, "Register read error: {}", msg),
            Self::RegisterWriteError(msg) => 
                write!(f, "Register write error: {}", msg),
            Self::InvalidInstruction { pc } => 
                write!(f, "Invalid instruction at PC 0x{:x}", pc),
            Self::UnsupportedInstruction { opcode } => 
                write!(f, "Unsupported instruction: {}", opcode),
            Self::ExecutionError(msg) => 
                write!(f, "Execution error: {}", msg),
            Self::InvalidHartId { hart_id } => 
                write!(f, "Invalid hart ID: {}", hart_id),
            Self::HartNotFound { hart_id } => 
                write!(f, "Hart not found: {}", hart_id),
            Self::ProgramLoadError(msg) => 
                write!(f, "Program load error: {}", msg),
            Self::EntryPointNotFound { program_id } => 
                write!(f, "Entry point not found for program {}", program_id),
            Self::DapError(msg) => 
                write!(f, "DAP error: {}", msg),
            Self::GeneralError(msg) => 
                write!(f, "{}", msg),
        }
    }
}