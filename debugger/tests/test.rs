use core_utils::filesystem::read_file_to_string;
use riscv_asm_lib::r5asm::assembler::*;
use std::path::PathBuf;

#[path = "../src/machine/mod.rs"]
mod machine;
#[path = "../src/memory/mod.rs"]
mod memory;
#[path = "../src/debugger_error.rs"]
mod debugger_error;

use crate::machine::machine::Machine;
use crate::machine::processor::Processor;

#[test]
fn test_machine_execution() {
    // Get the manifest directory and construct path to code.asm
       let asm_prog_text = r#"
.data
    a0: .word 0
    b0: .word 1

.text
main:
    add t0, t1, t2
    beq t3, t4, main
"#;
    
    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(&asm_prog_text).unwrap();
    machine.add_program(program);

    let t1 = machine.lookup_register("t1").unwrap();
    let t2 = machine.lookup_register("t2").unwrap();
    let t0 = machine.lookup_register("t0").unwrap();

    let default_hart_id = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(default_hart_id).unwrap();
    let pc = machine.get_default_hart().unwrap().pc;

    let hart = machine.get_default_hart_mut().unwrap();    
    hart.write_register(&t1, 10.into());
    hart.write_register(&t2, 20.into());
    

    let r = machine.step_hart(default_hart_id);
    let result = machine.get_default_hart().unwrap().read_register(&t0);
    
    assert!(r.is_ok());
    assert_eq!(result, 30.into());

    let r = machine.step_hart(default_hart_id);
    assert!(r.is_ok());

    let pc2 = machine.get_default_hart().unwrap().pc;
    assert_eq!(pc2, pc);
}

#[test]
fn test_data_load() {
    // Get the manifest directory and construct path to code.asm
    let asm_prog_text = r#"
.data
    a0: .word 0
    b0: .word 1

.text
main:
    la t0, b0
    lw t1, 0(t0)
"#;

    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(&asm_prog_text).unwrap();
    let prog_id = machine.add_program(program).unwrap();
    // `add_program` only caches instructions; `.data` must be written to the
    // processor's memory explicitly before `lw` can read the label.
    machine.load_program_to_processor(prog_id, 0, true).unwrap();

    let t1 = machine.lookup_register("t1").unwrap();
    let t2 = machine.lookup_register("t2").unwrap();
    let t0 = machine.lookup_register("t0").unwrap();

    let default_hart_id = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(default_hart_id).unwrap();
    let pc = machine.get_default_hart().unwrap().pc;

    let r = machine.step_hart(default_hart_id);
    assert!(r.is_ok());

    let r = machine.step_hart(default_hart_id);
    assert!(r.is_ok());

    let r = machine.step_hart(default_hart_id);
    assert!(r.is_ok());

    let has_inst = machine.has_inst_at_pc(default_hart_id);
    assert_eq!(has_inst, false);

    let result = machine.get_default_hart().unwrap().read_register(&t1);
    assert_eq!(result, 1.into());
}

#[test]
fn test_program_data_is_loaded_into_memory() {
    let asm_prog_text = r#"
.data
    a0: .word 0
    b0: .word 1

.text
main:
    la t0, b0
    lw t1, 0(t0)
"#;

    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(&asm_prog_text).unwrap();
    let prog_id = machine.add_program(program).unwrap();
    // load program into processor 0's local memory so data sections are accessible
    machine.load_program_to_processor(prog_id, 0, true).unwrap();

    assert_eq!(machine.processors[0].memory.read_u32(0).unwrap(), 0);
    assert_eq!(machine.processors[0].memory.read_u32(4).unwrap(), 1);
}

#[test]
fn test_jalr_uses_resolved_immediate() {
    let asm_prog_text = r#"
.text
main:
    addi t0, x0, 108
    jalr ra, 4(t0)
    addi t1, x0, 1
    addi t2, x0, 2
"#;

    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    machine.add_program(program);

    let t1 = machine.lookup_register("t1").unwrap();
    let t2 = machine.lookup_register("t2").unwrap();
    let ra = machine.lookup_register("ra").unwrap();

    let default_hart_id = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(default_hart_id).unwrap();

    machine.step_hart(default_hart_id).unwrap();
    machine.step_hart(default_hart_id).unwrap();
    machine.step_hart(default_hart_id).unwrap();

    let hart = machine.get_default_hart().unwrap();
    assert_eq!(hart.read_register(&t1), 0.into());
    assert_eq!(hart.read_register(&t2), 2.into());
    assert_eq!(hart.read_register(&ra), 108.into());
}

/// #2 stop-the-world semantics: single-stepping one hart must advance the
/// shared clock, but must NOT age the other harts' `elapsed_cycles`.
///
/// A second hart is required to observe this: with a single hart the frozen
/// and free-running models are indistinguishable.
#[test]
fn test_stop_the_world_single_step_freezes_other_harts() {
    let asm_prog_text = r#"
.text
main:
    add t0, t1, t2
    add t3, t4, t5
    add t6, t0, t1
"#;

    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    machine.add_program(program);

    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    // Give processor 0 a second hart so the freeze is observable.
    machine.add_more_harts_to_processor(0, 1).unwrap();
    let other = machine
        .get_all_harts()
        .iter()
        .map(|(_, h)| h.id)
        .find(|id| *id != hart0)
        .expect("a second hart should exist");
    machine.init_hart_to_entry_point(other).unwrap();

    let elapsed_of = |m: &Machine, id: u64| -> u64 {
        m.get_all_performances()
            .iter()
            .find(|p| p.hart_id == id)
            .expect("performance snapshot for hart")
            .elapsed_cycles
    };

    let clock_before = machine.get_global_clock();
    let hart0_before = elapsed_of(&machine, hart0);
    let other_before = elapsed_of(&machine, other);

    // Single-step ONLY hart0.
    machine.step_hart(hart0).unwrap();

    let clock_after = machine.get_global_clock();
    let hart0_after = elapsed_of(&machine, hart0);
    let other_after = elapsed_of(&machine, other);

    // Shared-clock invariant must still hold.
    assert!(
        clock_after > clock_before,
        "global clock must advance on single-step (shared clock invariant)"
    );
    // Stop-the-world: only the stepped hart ages.
    assert!(
        hart0_after > hart0_before,
        "the stepped hart must accumulate elapsed cycles"
    );
    assert_eq!(
        other_after, other_before,
        "the frozen hart must NOT age during another hart's single-step"
    );
}

/// Multi-hart integration: harts on the same processor must have isolated
/// registers and PCs, yet share one memory and one clock.
#[test]
fn test_multi_hart_isolated_registers_shared_memory() {
    let asm_prog_text = r#"
.text
main:
    lw t0, 0(t1)
    addi t2, x0, 7
"#;

    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();

    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    machine.add_more_harts_to_processor(0, 1).unwrap();
    let hart1 = machine
        .get_all_harts()
        .iter()
        .map(|(_, h)| h.id)
        .find(|id| *id != hart0)
        .expect("a second hart should exist");
    machine.init_hart_to_entry_point(hart1).unwrap();

    let t0 = machine.lookup_register("t0").unwrap();
    let t1 = machine.lookup_register("t1").unwrap();
    let test_addr: u64 = 0x1000;

    // Both harts must live on the same processor, hence share its memory+clock.
    assert_eq!(
        machine.get_processor_index(hart0),
        machine.get_processor_index(hart1),
        "both harts must belong to the same processor"
    );

    // 1. Isolated registers: a write on hart0 must not leak into hart1.
    machine
        .get_hart_mut(hart0)
        .unwrap()
        .write_register(&t1, test_addr.into())
        .unwrap();
    assert_eq!(
        machine.get_hart(hart0).unwrap().read_register(&t1),
        test_addr.into(),
        "hart0 should see its own write"
    );
    assert_eq!(
        machine.get_hart(hart1).unwrap().read_register(&t1),
        0.into(),
        "hart1 must NOT see hart0's register write"
    );

    // 2. Isolated PCs: stepping hart0 must not move hart1's PC.
    let pc0_before = machine.get_hart(hart0).unwrap().pc;
    let pc1_before = machine.get_hart(hart1).unwrap().pc;
    machine.step_hart(hart0).unwrap();
    assert_ne!(
        machine.get_hart(hart0).unwrap().pc,
        pc0_before,
        "hart0's PC should advance"
    );
    assert_eq!(
        machine.get_hart(hart1).unwrap().pc,
        pc1_before,
        "hart1's PC must not move when hart0 steps"
    );

    // 3. Shared memory: a value placed in processor 0's memory is visible to
    //    hart1 through its own load — proving both harts back onto one store.
    //    Use a positive word (MSB clear) so `lw`'s sign-extension leaves it
    //    unchanged and the assertion stays focused on memory sharing.
    machine
        .processors[0]
        .memory
        .write_u32(test_addr, 0x12345678)
        .unwrap();
    machine
        .get_hart_mut(hart1)
        .unwrap()
        .write_register(&t1, test_addr.into())
        .unwrap();
    machine.step_hart(hart1).unwrap(); // executes `lw t0, 0(t1)`
    assert_eq!(
        machine.get_hart(hart1).unwrap().read_register(&t0),
        0x12345678u64.into(),
        "hart1 must read what the shared processor memory holds"
    );
}

/// Multi-processor integration: each processor owns its own memory (isolated),
/// but all processors share one lockstep clock.
#[test]
fn test_multi_processor_isolated_memory_shared_clock() {
    let asm_prog_text = r#"
.text
main:
    addi t0, x0, 1
    addi t1, x0, 2
"#;

    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let prog_id = machine.add_program(program).unwrap();

    // Machine::default() created processor 0; add processor 1.
    machine.add_processor(Processor::default());
    assert_eq!(machine.processor_count(), 2, "expected two processors");

    // SMP: load the same program into every processor's own memory.
    for proc_idx in 0..machine.processor_count() {
        machine
            .load_program_to_processor(prog_id, proc_idx, true)
            .unwrap();
    }
    let hart_ids: Vec<u64> = machine.get_all_harts().iter().map(|(_, h)| h.id).collect();
    assert_eq!(hart_ids.len(), 2, "one hart per processor");
    for id in &hart_ids {
        machine.init_hart_to_entry_point(*id).unwrap();
    }

    // 1. Isolated memory: the same address holds different values per processor.
    let addr: u64 = 0x1000;
    machine.processors[0].memory.write_u32(addr, 0xDEADBEEF).unwrap();
    machine.processors[1].memory.write_u32(addr, 0xCAFEBABE).unwrap();
    assert_eq!(machine.processors[0].memory.read_u32(addr).unwrap(), 0xDEADBEEF);
    assert_eq!(
        machine.processors[1].memory.read_u32(addr).unwrap(),
        0xCAFEBABE,
        "processors must NOT share memory"
    );

    // 2. Shared lockstep clock: stepping one hart advances every processor's clock.
    let hart0 = machine.get_default_hart_id();
    assert_eq!(machine.get_processor_index(hart0), Some(0));

    let clock0_before = machine.get_processor_clock(0).unwrap();
    let clock1_before = machine.get_processor_clock(1).unwrap();
    assert_eq!(clock0_before, clock1_before, "clocks must start in lockstep");

    machine.step_hart(hart0).unwrap();

    let clock0_after = machine.get_processor_clock(0).unwrap();
    let clock1_after = machine.get_processor_clock(1).unwrap();
    assert!(clock0_after > clock0_before, "processor 0 clock must advance");
    assert_eq!(
        clock0_after, clock1_after,
        "processor 1 clock must advance identically (shared lockstep clock)"
    );
}
