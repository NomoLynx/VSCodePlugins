use core_utils::filesystem::read_file_to_string;
use riscv_asm_lib::r5asm::assembler::*;
use std::path::PathBuf;

#[path = "../src/machine/mod.rs"]
mod machine;
#[path = "../src/memory/mod.rs"]
mod memory;
#[path = "../src/debugger_error.rs"]
mod debugger_error;

use crate::machine::hart::{HartState, EXCEPTION_ENVIRONMENT_CALL_FROM_M};
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

/// exit semantics: a7=93 (exit) ends the calling hart with the a0 exit code.
/// Instructions after the ecall must NOT execute (t0 stays 0) and the PC
/// stays on the ecall (exit does not jump to a trap vector).
#[test]
fn test_exit_93_single_hart() {
    let asm_prog_text = r#"
.text
main:
    addi a7, x0, 93
    addi a0, x0, 7
    ecall
    addi t0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    machine.step_hart(hart0).unwrap(); // addi a7
    machine.step_hart(hart0).unwrap(); // addi a0
    let pc_at_ecall = machine.get_hart(hart0).unwrap().pc;
    machine.step_hart(hart0).unwrap(); // ecall -> exit

    let hart = machine.get_hart(hart0).unwrap();
    assert!(
        matches!(hart.state, HartState::Finished),
        "hart should be Finished after exit"
    );
    assert_eq!(hart.exit_code, Some(7), "exit code should come from a0");
    assert_eq!(hart.x.read(5), 0, "instruction after ecall must not execute");
    assert_eq!(hart.pc, pc_at_ecall, "exit leaves the PC on the ecall");
    assert_eq!(machine.session_exit_code(), 7);
}

/// exit semantics: a7=93 ends ONLY the calling hart; a sibling hart on the
/// same processor keeps running and carries no exit code.
#[test]
fn test_exit_93_multi_hart_only_own_hart() {
    let asm_prog_text = r#"
.text
main:
    addi a7, x0, 93
    addi a0, x0, 5
    ecall
    addi t0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    machine.add_more_harts_to_processor(0, 1).unwrap();

    let hart_ids: Vec<u64> = machine.get_all_harts().iter().map(|(_, h)| h.id).collect();
    assert_eq!(hart_ids.len(), 2, "expected two harts on processor 0");
    let (hart0, hart1) = (hart_ids[0], hart_ids[1]);
    machine.init_hart_to_entry_point(hart0).unwrap();
    machine.init_hart_to_entry_point(hart1).unwrap();

    for _ in 0..3 {
        machine.step_hart(hart0).unwrap();
    }
    machine.step_hart(hart1).unwrap(); // executes only the first addi

    let h0 = machine.get_hart(hart0).unwrap();
    let h1 = machine.get_hart(hart1).unwrap();
    assert!(
        matches!(h0.state, HartState::Finished),
        "caller hart must be Finished"
    );
    assert_eq!(h0.exit_code, Some(5), "caller carries the exit code");
    assert!(
        matches!(h1.state, HartState::Running),
        "sibling hart must keep running"
    );
    assert_eq!(h1.exit_code, None, "sibling hart carries no exit code");
}

/// exit semantics: a7=94 (exit_group) ends EVERY hart on EVERY processor.
/// Only the calling hart owns the exit code; the other harts finish without
/// one so the dump cannot mislead.
#[test]
fn test_exit_94_cross_processor() {
    let asm_prog_text = r#"
.text
main:
    addi a7, x0, 94
    addi a0, x0, 3
    ecall
    addi t0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    // Two processors, one hart each (SMP).
    machine.add_processor(Processor::default());
    for proc_idx in 0..machine.processor_count() {
        machine.load_program_to_processor(prog_id, proc_idx, true).unwrap();
    }
    let hart_ids: Vec<u64> = machine.get_all_harts().iter().map(|(_, h)| h.id).collect();
    assert_eq!(hart_ids.len(), 2, "expected one hart per processor");
    let (hart0, hart1) = (hart_ids[0], hart_ids[1]);
    machine.init_hart_to_entry_point(hart0).unwrap();
    machine.init_hart_to_entry_point(hart1).unwrap();

    for _ in 0..3 {
        machine.step_hart(hart0).unwrap();
    }

    for (_, h) in machine.get_all_harts() {
        assert!(
            matches!(h.state, HartState::Finished),
            "exit_group must finish every hart"
        );
    }
    assert_eq!(
        machine.get_hart(hart0).unwrap().exit_code,
        Some(3),
        "caller carries the exit code"
    );
    assert_eq!(
        machine.get_hart(hart1).unwrap().exit_code,
        None,
        "other harts carry no exit code"
    );
    assert_eq!(machine.session_exit_code(), 3);
}

/// Regression guard: an ecall that is NOT an exit (a7 != 93/94) keeps the
/// historical trap semantics (mcause = Environment call from M-mode) and is
/// never misrouted to the exit path (no exit_code is recorded).
#[test]
fn test_non_exit_ecall_traps_as_before() {
    let asm_prog_text = r#"
.text
main:
    addi a7, x0, 1
    ecall
    addi t0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    machine.step_hart(hart0).unwrap(); // addi a7
    machine.step_hart(hart0).unwrap(); // ecall -> trap

    let hart = machine.get_hart(hart0).unwrap();
    assert_eq!(
        hart.read_csr(0x342),
        EXCEPTION_ENVIRONMENT_CALL_FROM_M,
        "mcause must be Environment call from M-mode"
    );
    assert_eq!(hart.exit_code, None, "a non-exit ecall must not record an exit code");
}

/// dump_registers: processor/hart sections, general registers, core CSRs and
/// the mechanical float view (raw | low32 as f32 | full64 as f64 | encoding).
#[test]
fn test_dump_registers_format() {
    let asm_prog_text = r#"
.text
main:
    addi a0, x0, 42
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    {
        let hart = machine.get_hart_mut(hart0).unwrap();
        hart.x.write(10, 42); // a0 = 42
        // f0: NaN-boxed f32 1.0; f1: plain f64 1.0
        hart.f.write(0, 0xFFFFFFFF3F800000);
        hart.f.write(1, 0x3FF0000000000000);
    }

    let dir = std::env::temp_dir().join("rvdbg_dump_registers_test");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("dump.registers.txt");
    machine.dump_registers(&out, "dump_test").unwrap();
    let content = std::fs::read_to_string(&out).unwrap();

    assert!(content.contains("# Register Dump"), "missing file header");
    assert!(content.contains("## Processor 0"), "missing processor section");
    assert!(content.contains("### Hart 0"), "missing hart section");
    assert!(content.contains("#### General Registers"), "missing GPR section");
    assert!(content.contains("#### Float Registers"), "missing float section");
    assert!(content.contains("#### Core CSRs"), "missing CSR section");
    assert!(content.contains("x0"), "missing register name");
    assert!(content.contains("0x000000000000002a"), "a0=42 must appear in hex");
    assert!(content.contains("mstatus"), "core CSRs must be listed");
    assert!(content.contains("mcause"), "core CSRs must be listed");
    assert!(
        content.contains("0xffffffff3f800000"),
        "f0 raw 64-bit encoding must appear"
    );
    assert!(
        content.contains("0x3ff0000000000000"),
        "f1 raw 64-bit encoding must appear"
    );
    assert!(
        content.contains("NaN-boxed f32"),
        "f0 must be tagged NaN-boxed f32"
    );
    assert!(content.contains("plain f64"), "f1 must be tagged plain f64");
    // Numeric columns: f0's low32-as-f32 is 1.0 and f1's full64-as-f64 is 1.0.
    assert!(
        content.contains("1.000000"),
        "float columns must render the actual numeric values"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// session_exit_code: 0 before any exit, the exit code once a hart finished
/// with one, and 1 whenever any hart recorded an error (errors win).
#[test]
fn test_session_exit_code() {
    let asm_prog_text = r#"
.text
main:
    addi a7, x0, 93
    addi a0, x0, 5
    ecall
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    assert_eq!(machine.session_exit_code(), 0, "no exit yet -> 0");

    for _ in 0..3 {
        machine.step_hart(hart0).unwrap();
    }
    assert_eq!(machine.session_exit_code(), 5, "exit code must pass through");

    machine.get_hart_mut(hart0).unwrap().error_info = Some("boom".to_string());
    assert_eq!(machine.session_exit_code(), 1, "an error must win over exit codes");
}

/// Natural run-off (PC past the last instruction): the hart finishes with NO
/// exit code and NO error, so the session exit code is 0.
#[test]
fn test_run_off_ends_with_zero_exit_code() {
    let asm_prog_text = r#"
.text
main:
    addi t0, x0, 1
    addi t1, x0, 2
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    machine.step_hart(hart0).unwrap();
    machine.step_hart(hart0).unwrap();

    let hart = machine.get_hart(hart0).unwrap();
    assert!(
        matches!(hart.state, HartState::Finished),
        "running off the program must finish the hart"
    );
    assert_eq!(hart.exit_code, None, "run-off carries no exit code");
    assert_eq!(hart.error_info, None, "run-off is not an error");
    assert_eq!(machine.session_exit_code(), 0);
}

/// A real execution error (load from unmapped memory) must mark the hart
/// Finished with an error description and force session_exit_code = 1. The
/// register dump must label the hart "Finished (error: ...)".
#[test]
fn test_execution_error_sets_error_info_and_exit_code_1() {
    let asm_prog_text = r#"
.text
main:
    lw t0, 0(t1)
    addi t0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    // Default memory maps [0, 64 MiB); point t1 far beyond it.
    machine.get_hart_mut(hart0).unwrap().x.write(6, 0x8000_0000); // t1
    assert!(
        machine.step_hart(hart0).is_err(),
        "load from unmapped memory must fail"
    );

    let hart = machine.get_hart(hart0).unwrap();
    assert!(matches!(hart.state, HartState::Finished));
    assert!(hart.error_info.is_some(), "error_info must describe the fault");
    assert_eq!(hart.exit_code, None);
    assert_eq!(machine.session_exit_code(), 1, "errors force exit code 1");

    // The dump must label the error state.
    let dir = std::env::temp_dir().join("rvdbg_error_dump_test");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("err.registers.txt");
    machine.dump_registers(&out, "err_test").unwrap();
    let content = std::fs::read_to_string(&out).unwrap();
    assert!(
        content.contains("Finished (error:"),
        "dump must label error termination"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// Stop-the-world clock: once a hart is Finished (exit), its cycle counters
/// must stay frozen even as other harts keep executing and the shared clock
/// advances.
#[test]
fn test_finished_hart_cycles_frozen() {
    let asm_prog_text = r#"
.text
main:
    addi a7, x0, 93
    addi a0, x0, 7
    ecall
    addi t0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
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

    // hart0 exits.
    for _ in 0..3 {
        machine.step_hart(hart0).unwrap();
    }
    assert!(matches!(
        machine.get_hart(hart0).unwrap().state,
        HartState::Finished
    ));

    let perf0_before = machine.get_hart_performance(hart0).unwrap();
    let clock_before = machine.get_global_clock();

    // hart1 keeps executing; the shared clock must keep advancing.
    machine.step_hart(hart1).unwrap();
    machine.step_hart(hart1).unwrap();
    assert!(
        machine.get_global_clock() > clock_before,
        "shared clock must keep advancing after one hart finished"
    );

    let perf0_after = machine.get_hart_performance(hart0).unwrap();
    assert_eq!(
        perf0_before.elapsed_cycles, perf0_after.elapsed_cycles,
        "a Finished hart's elapsed_cycles must stay frozen"
    );
    assert_eq!(
        perf0_before.exec_cycles, perf0_after.exec_cycles,
        "a Finished hart's exec_cycles must stay frozen"
    );
    assert_eq!(perf0_before.inst_count, perf0_after.inst_count);
    assert_eq!(perf0_before.runtime_cycles, perf0_after.runtime_cycles);
}

/// Restart semantics: init_hart_to_entry_point clears exit_code, error_info
/// and finish_clock, and the hart can exit cleanly again afterwards.
#[test]
fn test_reset_clears_exit_state_and_re_exit_works() {
    let asm_prog_text = r#"
.text
main:
    addi a7, x0, 93
    addi a0, x0, 5
    ecall
    addi t0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    for _ in 0..3 {
        machine.step_hart(hart0).unwrap();
    }
    {
        let hart = machine.get_hart(hart0).unwrap();
        assert!(matches!(hart.state, HartState::Finished));
        assert_eq!(hart.exit_code, Some(5));
        assert!(
            hart.finish_clock.is_some(),
            "finish_clock must record the end time"
        );
    }

    machine.init_hart_to_entry_point(hart0).unwrap();
    {
        let hart = machine.get_hart(hart0).unwrap();
        assert!(matches!(hart.state, HartState::Running));
        assert_eq!(hart.exit_code, None, "reset must clear exit_code");
        assert_eq!(hart.error_info, None, "reset must clear error_info");
        assert_eq!(hart.finish_clock, None, "reset must clear finish_clock");
    }

    // Exit again with a different code.
    machine.step_hart(hart0).unwrap(); // addi a7
    machine.step_hart(hart0).unwrap(); // addi a0 = 5
    machine.get_hart_mut(hart0).unwrap().x.write(10, 9); // a0 = 9
    machine.step_hart(hart0).unwrap(); // ecall -> exit

    let hart = machine.get_hart(hart0).unwrap();
    assert!(matches!(hart.state, HartState::Finished));
    assert_eq!(hart.exit_code, Some(9), "second exit must capture the new code");
    assert_eq!(machine.session_exit_code(), 9);
}

/// Robustness: unknown hart ids must produce errors / None, never a panic.
#[test]
fn test_invalid_hart_id_returns_error_no_panic() {
    let asm_prog_text = r#"
.text
main:
    addi t0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    assert!(machine.step_hart(999).is_err(), "stepping an unknown hart must error");
    assert!(machine.init_hart_to_entry_point(999).is_err());
    assert!(machine.get_hart(999).is_none());
    assert!(machine.get_hart_mut(999).is_none());
    assert!(machine.get_hart_performance(999).is_none());
}

/// Exit-code edge cases: exit(0) records Some(0) yet resolves to session code
/// 0, and a negative a0 round-trips through the u64-to-i64 conversion.
#[test]
fn test_exit_zero_and_negative_exit_codes() {
    let asm_prog_text = r#"
.text
main:
    addi a7, x0, 93
    addi a0, x0, 5
    ecall
    addi t0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    // Phase 1: a0 = 0.
    machine.step_hart(hart0).unwrap(); // addi a7
    machine.step_hart(hart0).unwrap(); // addi a0
    machine.get_hart_mut(hart0).unwrap().x.write(10, 0); // a0 = 0
    machine.step_hart(hart0).unwrap(); // ecall -> exit
    assert_eq!(
        machine.get_hart(hart0).unwrap().exit_code,
        Some(0),
        "exit(0) is still recorded on the hart"
    );
    assert_eq!(machine.session_exit_code(), 0, "all-zero exits resolve to 0");

    // Phase 2: a0 = -1 (all bits set).
    machine.init_hart_to_entry_point(hart0).unwrap();
    machine.step_hart(hart0).unwrap();
    machine.step_hart(hart0).unwrap();
    machine.get_hart_mut(hart0).unwrap().x.write(10, u64::MAX); // a0 = -1
    machine.step_hart(hart0).unwrap(); // ecall -> exit
    assert_eq!(
        machine.get_hart(hart0).unwrap().exit_code,
        Some(-1),
        "a0 must sign-extend to i64"
    );
    assert_eq!(machine.session_exit_code(), -1);
}

/// exit_group symmetry: the caller may live on ANY processor — a hart on
/// processor 1 must still terminate every hart and own the exit code.
#[test]
fn test_exit_group_initiated_from_second_processor() {
    let asm_prog_text = r#"
.text
main:
    addi a7, x0, 94
    addi a0, x0, 3
    ecall
    addi t0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.add_processor(Processor::default());
    for proc_idx in 0..machine.processor_count() {
        machine.load_program_to_processor(prog_id, proc_idx, true).unwrap();
    }
    let hart_ids: Vec<u64> = machine.get_all_harts().iter().map(|(_, h)| h.id).collect();
    let (hart0, hart1) = (hart_ids[0], hart_ids[1]);
    machine.init_hart_to_entry_point(hart0).unwrap();
    machine.init_hart_to_entry_point(hart1).unwrap();

    // Processor 1's hart calls exit_group.
    for _ in 0..3 {
        machine.step_hart(hart1).unwrap();
    }

    for (_, h) in machine.get_all_harts() {
        assert!(
            matches!(h.state, HartState::Finished),
            "exit_group must finish every hart regardless of caller location"
        );
    }
    assert_eq!(
        machine.get_hart(hart1).unwrap().exit_code,
        Some(3),
        "the caller (processor 1's hart) owns the exit code"
    );
    assert_eq!(machine.get_hart(hart0).unwrap().exit_code, None);
    assert_eq!(machine.session_exit_code(), 3);
}

/// Full dump (always on): vector registers are emitted as raw byte hex, one
/// 512-bit register per line, together with the runtime vector state.
#[test]
fn test_dump_registers_full_vector_section() {
    let asm_prog_text = r#"
.text
main:
    addi a0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    {
        let hart = machine.get_hart_mut(hart0).unwrap();
        hart.v.write_bytes(0, &[0xde, 0xad, 0xbe, 0xef]);
        hart.vector_state.vl = 7;
        hart.vector_state.sew = 32;
        hart.vector_state.lmul = 2;
    }

    let dir = std::env::temp_dir().join("rvdbg_full_vector_test");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("vec.registers.txt");
    machine.dump_registers(&out, "vec_test").unwrap();
    let text = std::fs::read_to_string(&out).unwrap();

    assert!(text.contains("#### Vector Registers"), "vector section missing");
    assert!(text.contains("#### Vector State"), "vector state section missing");
    assert!(text.contains("0xdeadbeef"), "v0 raw bytes must appear as hex");
    assert!(text.contains("v31 "), "all 32 vector registers must be listed");
    assert!(text.contains("vl   7"), "runtime vl must appear");
    assert!(text.contains("sew  32 bits"), "runtime sew must appear");
    assert!(text.contains("lmul 2"), "runtime lmul must appear");

    std::fs::remove_dir_all(&dir).ok();
}

/// Full dump (always on): all implemented CSRs beyond the core seven are
/// emitted by name from a table aligned with `Hart::read_csr`.
#[test]
fn test_dump_registers_full_additional_csrs() {
    let asm_prog_text = r#"
.text
main:
    addi a0, x0, 1
"#;
    let program = parse_asm_use_default_config(asm_prog_text).unwrap();
    let mut machine = Machine::default();
    let prog_id = machine.add_program(program).unwrap();
    machine.load_program_to_processor(prog_id, 0, true).unwrap();
    let hart0 = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart0).unwrap();

    {
        let hart = machine.get_hart_mut(hart0).unwrap();
        hart.write_csr(0x301, 0x1122_3344_5566_7788); // misa
        hart.write_csr(0x180, 0x99aa_bbcc_ddee_ff00); // satp
        hart.write_csr(0x008, 5); // vstart
    }

    let dir = std::env::temp_dir().join("rvdbg_full_csr_test");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("csr.registers.txt");
    machine.dump_registers(&out, "csr_test").unwrap();
    let text = std::fs::read_to_string(&out).unwrap();

    assert!(text.contains("#### Core CSRs"), "core section must remain");
    assert!(text.contains("#### Additional CSRs"), "full CSR section missing");
    assert!(text.contains("misa") && text.contains("0x1122334455667788"));
    assert!(text.contains("satp") && text.contains("0x99aabbccddeeff00"));
    assert!(text.contains("vstart") && text.contains("(0x008)"));
    assert!(text.contains("vtype"), "vector CSR must be listed");
    assert!(text.contains("mscratch"), "machine CSR must be listed");
    assert!(text.contains("sepc"), "supervisor CSR must be listed");
    assert_eq!(
        text.matches("mstatus").count(),
        1,
        "core CSRs must not be duplicated"
    );

    std::fs::remove_dir_all(&dir).ok();
}
