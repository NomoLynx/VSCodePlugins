use super::hart::Hart;

pub enum AddressingMode {
    Bit32,
    Bit64,
    Bit128,
}

impl AddressingMode {
    pub fn to_ptr_size(&self) -> u64 {
        match self {
            AddressingMode::Bit32 => 4,
            AddressingMode::Bit64 => 8,
            AddressingMode::Bit128 => 16,
        }
    }
}

pub struct Processor {
    pub id: u64,
    pub harts: Vec<Hart>,
    pub addressing: AddressingMode,
}

impl Default for Processor {
    fn default() -> Self {
        Self {
            id: 0,
            harts: vec![Hart::default()],
            addressing: AddressingMode::Bit64,
        }
    }
}
