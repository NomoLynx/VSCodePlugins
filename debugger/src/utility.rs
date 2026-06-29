use riscv_asm_lib::r5asm::assembler::*;
use crate::machine::machine::Machine;
use crate::machine::runtime_value::RuntimeValue;

// ============================================================
// Shared helper functions for all test files
// ============================================================

/// Create a Machine, add the ASM program, and init the default hart to entry point.
pub fn setup_machine(asm_text: &str) -> Machine {
    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(asm_text).unwrap();
    machine.add_program(program);
    let hart_id = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart_id).unwrap();
    machine
}

/// Step the default hart by one instruction.
pub fn step(machine: &mut Machine) {
    let hart_id = machine.get_default_hart_id();
    machine.step_hart(hart_id).unwrap();
}

/// Read an integer register by name.
pub fn get_reg(machine: &Machine, name: &str) -> u64 {
    let reg = machine.lookup_register(name).unwrap();
    let hart = machine.get_default_hart().unwrap();
    match hart.read_register(&reg) {
        RuntimeValue::Integer(v) => v,
        _ => panic!("Expected integer register"),
    }
}

/// Write an integer register by name.
pub fn set_reg(machine: &mut Machine, name: &str, value: u64) {
    let reg = machine.lookup_register(name).unwrap();
    let hart = machine.get_default_hart_mut().unwrap();
    hart.write_register(&reg, RuntimeValue::Integer(value));
}

/// Read a float register by name.
pub fn get_freg(machine: &Machine, name: &str) -> f64 {
    let reg = machine.lookup_register(name).unwrap();
    let hart = machine.get_default_hart().unwrap();
    match hart.read_register(&reg) {
        RuntimeValue::Float64(v) => v,
        _ => panic!("Expected float register"),
    }
}

/// Write a float register by name.
pub fn set_freg(machine: &mut Machine, name: &str, value: f64) {
    let reg = machine.lookup_register(name).unwrap();
    let hart = machine.get_default_hart_mut().unwrap();
    hart.write_register(&reg, RuntimeValue::Float64(value));
}

/// Read a vector register by name.
pub fn get_vreg(machine: &Machine, name: &str) -> Vec<u8> {
    let reg = machine.lookup_register(name).unwrap();
    let hart = machine.get_default_hart().unwrap();
    match hart.read_register(&reg) {
        RuntimeValue::Vector(v) => v,
        _ => panic!("Expected vector register"),
    }
}

/// Write a vector register by name.
pub fn set_vreg(machine: &mut Machine, name: &str, bytes: Vec<u8>) {
    let reg = machine.lookup_register(name).unwrap();
    let hart = machine.get_default_hart_mut().unwrap();
    hart.write_register(&reg, RuntimeValue::Vector(bytes));
}

/// Read a u64 element from vector bytes at the given element index (0-based).
pub fn get_velem(vreg: &[u8], elem_idx: usize, sew: usize) -> u64 {
    let offset = elem_idx * sew;
    let mut val: u64 = 0;
    for b in 0..sew {
        if offset + b < vreg.len() {
            val |= (vreg[offset + b] as u64) << (b * 8);
        }
    }
    val
}

/// Set a u64 element in vector bytes at the given element index (0-based).
pub fn set_velem(vreg: &mut [u8], elem_idx: usize, sew: usize, value: u64) {
    let offset = elem_idx * sew;
    for b in 0..sew {
        if offset + b < vreg.len() {
            vreg[offset + b] = ((value >> (b * 8)) & 0xFF) as u8;
        }
    }
}

/// Setup VL and SEW by directly setting CSR values (simulates vsetvli).
pub fn setup_vsetvli(machine: &mut Machine, sew: &str, _lmul: &str, vl: u64) {
    let hart = machine.get_default_hart_mut().unwrap();
    let sew_enc = match sew {
        "e8" => 0u64,
        "e16" => 1,
        "e32" => 2,
        "e64" => 3,
        _ => 2,
    };
    hart.csr.vtype = sew_enc << 3; // lmul = m1 (0)
    hart.csr.vl = vl;
    hart.vector_state.vl = vl as usize;
    hart.vector_state.sew = 1 << (sew_enc as usize);
    hart.vector_state.lmul = 1;
}
