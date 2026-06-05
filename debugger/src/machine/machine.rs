use crate::memory::memory::Memory;
use super::processor::Processor;

pub struct Machine {
    pub memory: Memory,
    pub processors: Vec<Processor>,
}
