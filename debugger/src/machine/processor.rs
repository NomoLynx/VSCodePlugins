use super::hart::Hart;
use crate::memory::memory::Memory;

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
    pub harts: Vec<Hart>,
    pub addressing: AddressingMode,
    /// Processor-local memory shared by all harts within this processor.
    pub memory: Memory,
    /// Processor-local clock — advances in lockstep with all other processors.
    /// Private: modifications must go through step_all_harts or set_clock.
    clock: u64,
}

impl Processor {
    /// Return the current processor clock value (read-only).
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// Set the processor clock directly. Only accessible within the crate
    /// so that the Machine layer can enforce the global clock invariant.
    pub(crate) fn set_clock(&mut self, value: u64) {
        self.clock = value;
    }
}

impl Default for Processor {
    fn default() -> Self {
        Self {
            harts: vec![Hart::default()],
            addressing: AddressingMode::Bit64,
            memory: Memory::default(),
            clock: 0,
        }
    }
}
