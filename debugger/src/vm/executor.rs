use crate::machine::machine::Machine;

pub struct MachineExecutor {
    pub machine: Machine,
}

impl MachineExecutor {
    pub fn step_hart(&mut self, _hart_id: u64) {
        // execute one instruction
    }
}
