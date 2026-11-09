//! A processor: a hart container plus processor-local memory and a clock.

use super::hart::Hart;
use crate::memory::memory::Memory;

/// The address width of a processor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddressingMode {
    /// 32-bit addressing.
    Bit32,
    /// 64-bit addressing.
    Bit64,
    /// 128-bit addressing.
    Bit128,
}

impl AddressingMode {
    /// Returns the size in bytes of a pointer for this addressing mode.
    #[must_use]
    pub fn to_ptr_size(self) -> u64 {
        match self {
            Self::Bit32 => 4,
            Self::Bit64 => 8,
            Self::Bit128 => 16,
        }
    }
}

/// A logical processor: one or more harts sharing local memory and a clock.
#[derive(Debug)]
pub struct Processor {
    /// The harts belonging to this processor.
    pub(crate) harts: Vec<Hart>,
    /// The address width of this processor.
    pub(crate) addressing: AddressingMode,
    /// Processor-local memory shared by all harts in this processor.
    pub(crate) memory: Memory,
    /// Processor-local clock, advancing in lockstep across all processors.
    clock: u64,
}

impl Processor {
    /// Returns the current processor clock value (read-only).
    #[must_use]
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// Sets the processor clock directly.
    ///
    /// Only crate-visible so that the machine layer can enforce the global
    /// clock invariant.
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
