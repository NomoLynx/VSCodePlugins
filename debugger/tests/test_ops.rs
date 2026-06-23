use core_utils::filesystem::read_file_to_string;
use riscv_asm_lib::r5asm::assembler::*;
use std::path::PathBuf;

#[path = "../src/machine/mod.rs"]
mod machine;
#[path = "../src/memory/mod.rs"]
mod memory;
#[path = "../src/vm/mod.rs"]
mod vm;
#[path = "../src/trace/mod.rs"]
mod trace;
#[path = "../src/debug/mod.rs"]
mod debug;
#[path = "../src/debugger_error.rs"]
mod debugger_error;

use crate::machine::machine::Machine;
use crate::machine::runtime_value::RuntimeValue;

// Helper function to create a machine with a given ASM program
fn setup_machine(asm_text: &str) -> Machine {
    let mut machine = Machine::default();
    let program = parse_asm_use_default_config(asm_text).unwrap();
    machine.add_program(program);
    let hart_id = machine.get_default_hart_id();
    machine.init_hart_to_entry_point(hart_id).unwrap();
    machine
}

fn step(machine: &mut Machine) {
    let hart_id = machine.get_default_hart_id();
    machine.step_hart(hart_id).unwrap();
}

fn get_reg(machine: &Machine, name: &str) -> u64 {
    let reg = machine.lookup_register(name).unwrap();
    let hart = machine.get_default_hart().unwrap();
    match hart.read_register(&reg) {
        RuntimeValue::Integer(v) => v,
        _ => panic!("Expected integer register"),
    }
}

fn get_freg(machine: &Machine, name: &str) -> f64 {
    let reg = machine.lookup_register(name).unwrap();
    let hart = machine.get_default_hart().unwrap();
    match hart.read_register(&reg) {
        RuntimeValue::Float64(v) => v,
        _ => panic!("Expected float register"),
    }
}

fn set_reg(machine: &mut Machine, name: &str, value: u64) {
    let reg = machine.lookup_register(name).unwrap();
    let hart = machine.get_default_hart_mut().unwrap();
    hart.write_register(&reg, RuntimeValue::Integer(value));
}

fn set_freg(machine: &mut Machine, name: &str, value: f64) {
    let reg = machine.lookup_register(name).unwrap();
    let hart = machine.get_default_hart_mut().unwrap();
    hart.write_register(&reg, RuntimeValue::Float64(value));
}

// ============================================================
// RV32M: Multiply/Divide tests
// ============================================================

#[test]
fn test_mulh() {
    // mulh t0, t1, t2  => t0 = (t1 * t2) >> 64 (signed)
    let asm = r#"
.text
main:
    mulh t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    // 0x8000000000000000 (i64::MIN) * 2 = 0xF000000000000000 (upper 64 bits = 0xFFFFFFFFFFFFFFFF)
    set_reg(&mut m, "t1", 0x8000000000000000u64);
    set_reg(&mut m, "t2", 2u64);
    step(&mut m);
    // i64::MIN * 2 = -2^63 * 2 = -2^64, upper 64 bits = -1 = 0xFFFFFFFFFFFFFFFF
    assert_eq!(get_reg(&m, "t0"), u64::MAX);
}

#[test]
fn test_mulhsu() {
    // mulhsu t0, t1, t2 => signed * unsigned, upper 64 bits
    let asm = r#"
.text
main:
    mulhsu t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    // -1 (i64) * 1 = 0xFFFFFFFFFFFFFFFF, upper = 0xFFFFFFFFFFFFFFFF
    set_reg(&mut m, "t1", u64::MAX); // -1 as signed
    set_reg(&mut m, "t2", 1u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), u64::MAX);
}

#[test]
fn test_mulhu() {
    // mulhu t0, t1, t2 => unsigned * unsigned, upper 64 bits
    let asm = r#"
.text
main:
    mulhu t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    // 0xFFFFFFFFFFFFFFFF * 2 = 0x1FFFFFFFFFFFFFFFE, upper = 1
    set_reg(&mut m, "t1", u64::MAX);
    set_reg(&mut m, "t2", 2u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_div() {
    // div t0, t1, t2
    let asm = r#"
.text
main:
    div t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 100u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, 14); // 100 / 7 = 14
}

#[test]
fn test_div_negative() {
    let asm = r#"
.text
main:
    div t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (-100i64) as u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, -14); // -100 / 7 = -14 (truncated toward 0)
}

#[test]
fn test_div_by_zero() {
    let asm = r#"
.text
main:
    div t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 100u64);
    set_reg(&mut m, "t2", 0u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), u64::MAX); // div by 0 = -1
}

#[test]
fn test_div_overflow() {
    // i64::MIN / -1 overflows => returns i64::MIN
    let asm = r#"
.text
main:
    div t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (i64::MIN as u64));
    set_reg(&mut m, "t2", (-1i64) as u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), i64::MIN as u64);
}

#[test]
fn test_divu() {
    let asm = r#"
.text
main:
    divu t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 100u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 14);
}

#[test]
fn test_divu_by_zero() {
    let asm = r#"
.text
main:
    divu t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 100u64);
    set_reg(&mut m, "t2", 0u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), u64::MAX);
}

#[test]
fn test_rem() {
    let asm = r#"
.text
main:
    rem t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 100u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, 2); // 100 % 7 = 2
}

#[test]
fn test_rem_negative() {
    let asm = r#"
.text
main:
    rem t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (-100i64) as u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, -2); // -100 % 7 = -2
}

#[test]
fn test_rem_by_zero() {
    let asm = r#"
.text
main:
    rem t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 100u64);
    set_reg(&mut m, "t2", 0u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 100); // rem by 0 = dividend
}

#[test]
fn test_remu() {
    let asm = r#"
.text
main:
    remu t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 100u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 2);
}

// ============================================================
// RV64I: Word operations tests
// ============================================================

#[test]
fn test_addiw() {
    let asm = r#"
.text
main:
    addiw t0, t1, 5
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10u64);
    step(&mut m);
    // 10 + 5 = 15, sign-extended from 32-bit
    assert_eq!(get_reg(&m, "t0"), 15);
}

#[test]
fn test_addiw_sign_extend() {
    // -1 + 0 = -1 as i32, sign-extended to i64
    let asm = r#"
.text
main:
    addiw t0, t1, 0
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFFu64); // all ones in lower 32 bits
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), u64::MAX); // sign extended to all 1s
}

#[test]
fn test_slliw() {
    let asm = r#"
.text
main:
    slliw t0, t1, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 3u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 12); // 3 << 2 = 12, sign-extended from 32-bit
}

#[test]
fn test_srliw() {
    let asm = r#"
.text
main:
    srliw t0, t1, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFFu64);
    step(&mut m);
    // logical shift: 0xFFFFFFFF >> 2 = 0x3FFFFFFF, zero-extended to u64
    assert_eq!(get_reg(&m, "t0"), 0x3FFFFFFF);
}

#[test]
fn test_sraiw() {
    let asm = r#"
.text
main:
    sraiw t0, t1, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFFu64); // -1 as i32
    step(&mut m);
    // arithmetic shift: -1 >> 2 = -1, sign-extended
    assert_eq!(get_reg(&m, "t0"), u64::MAX);
}

#[test]
fn test_addw() {
    let asm = r#"
.text
main:
    addw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFFu64);
    set_reg(&mut m, "t2", 1u64);
    step(&mut m);
    // -1 + 1 = 0 as i32, sign-extended = 0
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_subw() {
    let asm = r#"
.text
main:
    subw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10u64);
    set_reg(&mut m, "t2", 3u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 7);
}

#[test]
fn test_sllw() {
    let asm = r#"
.text
main:
    sllw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5u64);
    set_reg(&mut m, "t2", 3u64); // shift amount = 3 (only bottom 5 bits)
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 40); // 5 << 3 = 40
}

#[test]
fn test_srlw() {
    let asm = r#"
.text
main:
    srlw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFFu64);
    set_reg(&mut m, "t2", 4u64);
    step(&mut m);
    // logical shift: 0xFFFFFFFF >> 4 = 0x0FFFFFFF, zero-extended
    assert_eq!(get_reg(&m, "t0"), 0x0FFFFFFF);
}

#[test]
fn test_sraw() {
    let asm = r#"
.text
main:
    sraw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFFu64); // -1 as i32
    set_reg(&mut m, "t2", 4u64);
    step(&mut m);
    // arithmetic shift: -1 >> 4 = -1, sign-extended
    assert_eq!(get_reg(&m, "t0"), u64::MAX);
}

// ============================================================
// RV64M: Word multiply/divide tests
// ============================================================

#[test]
fn test_mulw() {
    let asm = r#"
.text
main:
    mulw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 6u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 42);
}

#[test]
fn test_mulw_sign_extend() {
    let asm = r#"
.text
main:
    mulw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    // 0x10000 * 0x10000 = 0x100000000, lower 32 = 0, truncated
    set_reg(&mut m, "t1", 0x10000u64);
    set_reg(&mut m, "t2", 0x10000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0); // overflow in 32-bit
}

#[test]
fn test_divw() {
    let asm = r#"
.text
main:
    divw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 100u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 14);
}

#[test]
fn test_divw_negative() {
    let asm = r#"
.text
main:
    divw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (-100i32 as i64) as u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, -14);
}

#[test]
fn test_divuw() {
    let asm = r#"
.text
main:
    divuw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 100u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 14);
}

#[test]
fn test_remw() {
    let asm = r#"
.text
main:
    remw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 100u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 2);
}

#[test]
fn test_remuw() {
    let asm = r#"
.text
main:
    remuw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 100u64);
    set_reg(&mut m, "t2", 7u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 2);
}

// ============================================================
// Zbb: Basic bit manipulation tests
// ============================================================

#[test]
fn test_andn() {
    let asm = r#"
.text
main:
    andn t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFu64);
    set_reg(&mut m, "t2", 0x0Fu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xF0); // t1 & ~t2
}

#[test]
fn test_orn() {
    let asm = r#"
.text
main:
    orn t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xF0u64);
    set_reg(&mut m, "t2", 0x0Fu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xFFFFFFFFFFFFFFF0); // t1 | ~t2
}

#[test]
fn test_xnor() {
    let asm = r#"
.text
main:
    xnor t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFu64);
    set_reg(&mut m, "t2", 0xF0u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xFFFFFFFFFFFFFFF0); // ~(t1 ^ t2)
}

#[test]
fn test_rol() {
    let asm = r#"
.text
main:
    rol t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x8000000000000001u64);
    set_reg(&mut m, "t2", 1u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 3); // rotate left by 1
}

#[test]
fn test_ror() {
    let asm = r#"
.text
main:
    ror t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x8000000000000001u64);
    set_reg(&mut m, "t2", 1u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xC000000000000000u64); // rotate right by 1
}

#[test]
fn test_rori() {
    let asm = r#"
.text
main:
    rori t0, t1, 1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x8000000000000001u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xC000000000000000u64); // rotate right by 1
}

#[test]
fn test_clz() {
    let asm = r#"
.text
main:
    clz t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0000000000000FFFu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 52); // leading zeros
}

#[test]
fn test_ctz() {
    let asm = r#"
.text
main:
    ctz t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x8000000000000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 63); // trailing zeros
}

#[test]
fn test_cpop() {
    let asm = r#"
.text
main:
    cpop t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x8000000000000001u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 2); // 2 bits set
}

#[test]
fn test_sextb() {
    let asm = r#"
.text
main:
    sext.b t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFu64); // -1 as i8 (all 8 bits set)
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), u64::MAX); // sign-extended to all 1s
}

#[test]
fn test_sexth() {
    let asm = r#"
.text
main:
    sext.h t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x8000u64); // -32768 as i16
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, -32768);
}

#[test]
fn test_zexth() {
    let asm = r#"
.text
main:
    zext.h t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFFFFFFu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xFFFF);
}

#[test]
fn test_rev8() {
    let asm = r#"
.text
main:
    rev8 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0102030405060708u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0807060504030201u64); // byte-reversed
}

#[test]
fn test_orcb() {
    let asm = r#"
.text
main:
    orc.b t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0001000200030004u64);
    step(&mut m);
    // each byte that is non-zero becomes 0xFF: bytes 0,2,4,6 are non-zero
    assert_eq!(get_reg(&m, "t0"), 0x00FF00FF00FF00FFu64);
}

// ============================================================
// Zbs: Single-bit instructions tests
// ============================================================

#[test]
fn test_bclr() {
    let asm = r#"
.text
main:
    bclr t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFu64);
    set_reg(&mut m, "t2", 0u64); // clear bit 0
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xFE);
}

#[test]
fn test_bclri() {
    let asm = r#"
.text
main:
    bclri t0, t1, 0
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xFE);
}

#[test]
fn test_bset() {
    let asm = r#"
.text
main:
    bset t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0u64);
    set_reg(&mut m, "t2", 5u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x20); // set bit 5
}

#[test]
fn test_bseti() {
    let asm = r#"
.text
main:
    bseti t0, t1, 5
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x20);
}

#[test]
fn test_bext() {
    let asm = r#"
.text
main:
    bext t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x20u64); // bit 5 set
    set_reg(&mut m, "t2", 5u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_bexti() {
    let asm = r#"
.text
main:
    bexti t0, t1, 5
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x20u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_binv() {
    let asm = r#"
.text
main:
    binv t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0Fu64); // bits 0-3 set
    set_reg(&mut m, "t2", 1u64);   // invert bit 1
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0D); // 1111 -> 1101
}

#[test]
fn test_binvi() {
    let asm = r#"
.text
main:
    binvi t0, t1, 1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0Fu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0D);
}

// ============================================================
// Min/Max tests
// ============================================================

#[test]
fn test_min() {
    let asm = r#"
.text
main:
    min t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (-5i64) as u64);
    set_reg(&mut m, "t2", 3u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, -5);
}

#[test]
fn test_minu() {
    let asm = r#"
.text
main:
    minu t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5u64);
    set_reg(&mut m, "t2", 3u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 3);
}

#[test]
fn test_max() {
    let asm = r#"
.text
main:
    max t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (-5i64) as u64);
    set_reg(&mut m, "t2", 3u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, 3);
}

#[test]
fn test_maxu() {
    let asm = r#"
.text
main:
    maxu t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5u64);
    set_reg(&mut m, "t2", 3u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 5);
}

// ============================================================
// Word bit operations
// ============================================================

#[test]
fn test_clzw() {
    let asm = r#"
.text
main:
    clzw t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x00000FFFu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 20); // leading zeros in lower 32 bits
}

#[test]
fn test_ctzw() {
    let asm = r#"
.text
main:
    ctzw t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x80000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 31);
}

#[test]
fn test_cpopw() {
    let asm = r#"
.text
main:
    cpopw t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x80000001u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 2);
}

#[test]
fn test_rolw() {
    let asm = r#"
.text
main:
    rolw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x80000001u64);
    set_reg(&mut m, "t2", 1u64);
    step(&mut m);
    // rotate left by 1 in 32-bit: 0x80000001 -> 0x00000003
    assert_eq!(get_reg(&m, "t0"), 3);
}

#[test]
fn test_rorw() {
    let asm = r#"
.text
main:
    rorw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x80000001u64);
    set_reg(&mut m, "t2", 1u64);
    step(&mut m);
    // rotate right by 1 in 32-bit: 0x80000001 -> 0xC0000000, sign-extended
    assert_eq!(get_reg(&m, "t0"), 0xFFFFFFFFC0000000u64);
}

#[test]
fn test_roriw() {
    let asm = r#"
.text
main:
    roriw t0, t1, 1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x80000001u64);
    step(&mut m);
    // sign-extended from 32-bit
    assert_eq!(get_reg(&m, "t0"), 0xFFFFFFFFC0000000u64);
}

// ============================================================
// Zba: Address generation tests
// ============================================================

#[test]
fn test_sh1add() {
    let asm = r#"
.text
main:
    sh1add t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5u64);
    set_reg(&mut m, "t2", 10u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 20); // (5 << 1) + 10 = 20
}

#[test]
fn test_sh2add() {
    let asm = r#"
.text
main:
    sh2add t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5u64);
    set_reg(&mut m, "t2", 10u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 30); // (5 << 2) + 10 = 30
}

#[test]
fn test_sh3add() {
    let asm = r#"
.text
main:
    sh3add t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5u64);
    set_reg(&mut m, "t2", 10u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 50); // (5 << 3) + 10 = 50
}

#[test]
fn test_slliuw() {
    let asm = r#"
.text
main:
    slli.uw t0, t1, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 3u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 12); // zero-extend lower 32, then shift
}

#[test]
fn test_adduw() {
    let asm = r#"
.text
main:
    add.uw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFFu64); // zero-extended = 0xFFFFFFFF
    set_reg(&mut m, "t2", 1u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x100000000);
}

#[test]
fn test_sh1adduw() {
    let asm = r#"
.text
main:
    sh1add.uw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5u64);
    set_reg(&mut m, "t2", 10u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 20); // (5 << 1) + 10 = 20
}

#[test]
fn test_sh2adduw() {
    let asm = r#"
.text
main:
    sh2add.uw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5u64);
    set_reg(&mut m, "t2", 10u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 30); // (5 << 2) + 10 = 30
}

#[test]
fn test_sh3adduw() {
    let asm = r#"
.text
main:
    sh3add.uw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5u64);
    set_reg(&mut m, "t2", 10u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 50); // (5 << 3) + 10 = 50
}

// ============================================================
// Zbkc: Carry-less multiply tests
// ============================================================

#[test]
fn test_clmul() {
    let asm = r#"
.text
main:
    clmul t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0b11u64); // 3
    set_reg(&mut m, "t2", 0b101u64); // 5
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0b1111); // GF(2) multiply: 3 * 5 = 15
}

#[test]
fn test_clmulr() {
    let asm = r#"
.text
main:
    clmulr t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    // clmulr: reversed carry-less multiply
    // rhs=1 means only bit 0 is set, result = lhs >> 63
    set_reg(&mut m, "t1", 0x8000000000000001u64); // bits 63 and 0 set
    set_reg(&mut m, "t2", 1u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1u64); // lhs >> 63 = 1
}

#[test]
fn test_clmulh() {
    let asm = r#"
.text
main:
    clmulh t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    // clmulh: high carry-less multiply, i from 1..=64
    // lhs=0x8000000000000000 (bit 63 set), rhs=1 (bit 0 set)
    // i=1: rhs>>1==0, skip. No bit triggers.
    set_reg(&mut m, "t1", 0x8000000000000000u64); // bit 63 set
    set_reg(&mut m, "t2", 1u64); // only bit 0 set, i=1 -> rhs>>1=0
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0u64);
}

// ============================================================
// Zicond: Conditional zero tests
// ============================================================

#[test]
fn test_czeroeqz_nonzero() {
    let asm = r#"
.text
main:
    czero.eqz t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 42u64);
    set_reg(&mut m, "t2", 1u64); // non-zero, so t0 = t1
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 42);
}

#[test]
fn test_czeroeqz_zero() {
    let asm = r#"
.text
main:
    czero.eqz t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 42u64);
    set_reg(&mut m, "t2", 0u64); // zero, so t0 = 0
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_czeronez_nonzero() {
    let asm = r#"
.text
main:
    czero.nez t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 42u64);
    set_reg(&mut m, "t2", 1u64); // non-zero, so t0 = 0
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_czeronez_zero() {
    let asm = r#"
.text
main:
    czero.nez t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 42u64);
    set_reg(&mut m, "t2", 0u64); // zero, so t0 = t1
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 42);
}

// ============================================================
// Floating-point tests
// ============================================================

#[test]
fn test_fadds() {
    let asm = r#"
.text
main:
    fadd.s f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.5f32 as f64);
    set_freg(&mut m, "f2", 3.5f32 as f64);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 6.0).abs() < 0.0001);
}

#[test]
fn test_fsubs() {
    let asm = r#"
.text
main:
    fsub.s f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 5.0f32 as f64);
    set_freg(&mut m, "f2", 3.0f32 as f64);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 2.0).abs() < 0.0001);
}

#[test]
fn test_fmuls() {
    let asm = r#"
.text
main:
    fmul.s f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.5f32 as f64);
    set_freg(&mut m, "f2", 4.0f32 as f64);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 10.0).abs() < 0.0001);
}

#[test]
fn test_fdivs() {
    let asm = r#"
.text
main:
    fdiv.s f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 6.0f32 as f64);
    set_freg(&mut m, "f2", 3.0f32 as f64);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 2.0).abs() < 0.0001);
}

#[test]
fn test_faddd() {
    let asm = r#"
.text
main:
    fadd.d f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.5);
    set_freg(&mut m, "f2", 3.5);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 6.0).abs() < 0.0001);
}

#[test]
fn test_fsubd() {
    let asm = r#"
.text
main:
    fsub.d f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 5.0);
    set_freg(&mut m, "f2", 3.0);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 2.0).abs() < 0.0001);
}

#[test]
fn test_fmuld() {
    let asm = r#"
.text
main:
    fmul.d f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.5);
    set_freg(&mut m, "f2", 4.0);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 10.0).abs() < 0.0001);
}

#[test]
fn test_fdivd() {
    let asm = r#"
.text
main:
    fdiv.d f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 6.0);
    set_freg(&mut m, "f2", 3.0);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 2.0).abs() < 0.0001);
}

#[test]
fn test_fsqrts() {
    let asm = r#"
.text
main:
    fsqrt.s f0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 4.0);
    step(&mut m);
    let result = get_freg(&m, "f0");
    assert!((result - 2.0).abs() < 0.0001);
}

#[test]
fn test_fsqrtd() {
    let asm = r#"
.text
main:
    fsqrt.d f0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 16.0);
    step(&mut m);
    let result = get_freg(&m, "f0");
    assert!((result - 4.0).abs() < 0.0001);
}

#[test]
fn test_fsgnjs() {
    let asm = r#"
.text
main:
    fsgnj.s f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);  // positive
    set_freg(&mut m, "f2", -5.0); // negative
    step(&mut m);
    assert!(get_freg(&m, "f0") < 0.0); // sign from f2
}

#[test]
fn test_fsgnjns() {
    let asm = r#"
.text
main:
    fsgnjn.s f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);  // positive
    set_freg(&mut m, "f2", -5.0); // negative
    step(&mut m);
    assert!(get_freg(&m, "f0") > 0.0); // negated sign of f2
}

#[test]
fn test_fsgnjxs() {
    let asm = r#"
.text
main:
    fsgnjx.s f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);  // positive
    set_freg(&mut m, "f2", -5.0); // negative
    step(&mut m);
    assert!(get_freg(&m, "f0") < 0.0); // xor of sign bits = negative
}

#[test]
fn test_fsgnjd() {
    let asm = r#"
.text
main:
    fsgnj.d f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);  // positive
    set_freg(&mut m, "f2", -5.0); // negative
    step(&mut m);
    assert!(get_freg(&m, "f0") < 0.0); // sign from f2
}

#[test]
fn test_fsgnjnd() {
    let asm = r#"
.text
main:
    fsgnjn.d f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);  // positive
    set_freg(&mut m, "f2", -5.0); // negative
    step(&mut m);
    assert!(get_freg(&m, "f0") > 0.0); // negated sign of f2
}

#[test]
fn test_fsgnjxd() {
    let asm = r#"
.text
main:
    fsgnjx.d f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);  // positive
    set_freg(&mut m, "f2", -5.0); // negative
    step(&mut m);
    assert!(get_freg(&m, "f0") < 0.0); // xor of sign bits = negative
}

#[test]
fn test_fmins() {
    let asm = r#"
.text
main:
    fmin.s f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);
    set_freg(&mut m, "f2", 5.0);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 3.0).abs() < 0.0001);
}

#[test]
fn test_fmaxs() {
    let asm = r#"
.text
main:
    fmax.s f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);
    set_freg(&mut m, "f2", 5.0);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 5.0).abs() < 0.0001);
}

#[test]
fn test_fmind() {
    let asm = r#"
.text
main:
    fmin.d f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);
    set_freg(&mut m, "f2", 5.0);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 3.0).abs() < 0.0001);
}

#[test]
fn test_fmaxd() {
    let asm = r#"
.text
main:
    fmax.d f0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);
    set_freg(&mut m, "f2", 5.0);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 5.0).abs() < 0.0001);
}

#[test]
fn test_fcvtsd() {
    let asm = r#"
.text
main:
    fcvt.s.d f0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.14f32 as f64);
    step(&mut m);
    let result = get_freg(&m, "f0");
    assert!((result - 3.14).abs() < 0.001);
}

#[test]
fn test_fcvtds() {
    let asm = r#"
.text
main:
    fcvt.d.s f0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.14159265358979);
    step(&mut m);
    let result = get_freg(&m, "f0");
    assert!((result - 3.14159265358979f64 as f32 as f64).abs() < 0.001);
}

#[test]
fn test_fcvtws() {
    let asm = r#"
.text
main:
    fcvt.w.s t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", -3.7f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, -3);
}

#[test]
fn test_fcvtwus() {
    let asm = r#"
.text
main:
    fcvt.wu.s t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.7f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 3);
}

#[test]
fn test_fcvtsw() {
    let asm = r#"
.text
main:
    fcvt.s.w f0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (-5i32 as i64) as u64);
    step(&mut m);
    assert!((get_freg(&m, "f0") + 5.0).abs() < 0.0001);
}

#[test]
fn test_fcvtswu() {
    let asm = r#"
.text
main:
    fcvt.s.wu f0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5u64);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 5.0).abs() < 0.0001);
}

#[test]
fn test_fcvtwd() {
    let asm = r#"
.text
main:
    fcvt.w.d t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", -3.7);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, -3);
}

#[test]
fn test_fcvtdw() {
    let asm = r#"
.text
main:
    fcvt.d.w f0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (-5i32 as i64) as u64);
    step(&mut m);
    assert!((get_freg(&m, "f0") + 5.0).abs() < 0.0001);
}

#[test]
fn test_fmvxw() {
    let asm = r#"
.text
main:
    fmv.x.s t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 1.0f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x3F800000u64); // IEEE 754 for 1.0f32
}

#[test]
fn test_fmvwx() {
    let asm = r#"
.text
main:
    fmv.s.x f0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x3F800000u64); // IEEE 754 for 1.0f32
    step(&mut m);
    assert!((get_freg(&m, "f0") - 1.0).abs() < 0.0001);
}

#[test]
fn test_fclasss_positive_normal() {
    let asm = r#"
.text
main:
    fclass.s t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1 << 6); // positive normal
}

#[test]
fn test_fclasss_negative_normal() {
    let asm = r#"
.text
main:
    fclass.s t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", (-3.0f32) as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1 << 1); // negative normal
}

#[test]
fn test_fclasss_zero() {
    let asm = r#"
.text
main:
    fclass.s t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 0.0f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1 << 4); // positive zero
}

#[test]
fn test_fclassd_positive_normal() {
    let asm = r#"
.text
main:
    fclass.d t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1 << 6); // positive normal
}

#[test]
fn test_fclassd_negative_normal() {
    let asm = r#"
.text
main:
    fclass.d t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", -3.0);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1 << 1); // negative normal
}

// ============================================================
// Float long conversion tests
// ============================================================

#[test]
fn test_fcvtls() {
    let asm = r#"
.text
main:
    fcvt.l.s t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 42.0f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 42);
}

#[test]
fn test_fcvtlus() {
    let asm = r#"
.text
main:
    fcvt.lu.s t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 42.0f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 42);
}

#[test]
fn test_fcvtsl() {
    let asm = r#"
.text
main:
    fcvt.s.l f0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (-42i64) as u64);
    step(&mut m);
    assert!((get_freg(&m, "f0") + 42.0).abs() < 0.001);
}

#[test]
fn test_fcvtslu() {
    let asm = r#"
.text
main:
    fcvt.s.lu f0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 42u64);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 42.0).abs() < 0.001);
}

#[test]
fn test_fcvtld() {
    let asm = r#"
.text
main:
    fcvt.l.d t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 42.0);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 42);
}

#[test]
fn test_fcvtlud() {
    let asm = r#"
.text
main:
    fcvt.lu.d t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 42.0);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 42);
}

#[test]
fn test_fcvtdl() {
    let asm = r#"
.text
main:
    fcvt.d.l f0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (-42i64) as u64);
    step(&mut m);
    assert!((get_freg(&m, "f0") + 42.0).abs() < 0.001);
}

#[test]
fn test_fcvtdlu() {
    let asm = r#"
.text
main:
    fcvt.d.lu f0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 42u64);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 42.0).abs() < 0.001);
}

// ============================================================
// Float move (double) tests
// ============================================================

#[test]
fn test_fmvdx() {
    let asm = r#"
.text
main:
    fmv.d.x f0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x3FF0000000000000u64); // IEEE 754 for 1.0
    step(&mut m);
    assert!((get_freg(&m, "f0") - 1.0).abs() < 0.0001);
}

#[test]
fn test_fmvxd() {
    let asm = r#"
.text
main:
    fmv.x.d t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 1.0);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x3FF0000000000000u64); // IEEE 754 for 1.0
}

// ============================================================
// Float comparison tests
// ============================================================

#[test]
fn test_feqs_equal() {
    let asm = r#"
.text
main:
    feq.s t0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0f32 as f64);
    set_freg(&mut m, "f2", 3.0f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_feqs_not_equal() {
    let asm = r#"
.text
main:
    feq.s t0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0f32 as f64);
    set_freg(&mut m, "f2", 4.0f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_flts_less() {
    let asm = r#"
.text
main:
    flt.s t0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.0f32 as f64);
    set_freg(&mut m, "f2", 3.0f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_flts_not_less() {
    let asm = r#"
.text
main:
    flt.s t0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0f32 as f64);
    set_freg(&mut m, "f2", 2.0f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_fles_less_or_equal() {
    let asm = r#"
.text
main:
    fle.s t0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0f32 as f64);
    set_freg(&mut m, "f2", 3.0f32 as f64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_feqd_equal() {
    let asm = r#"
.text
main:
    feq.d t0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);
    set_freg(&mut m, "f2", 3.0);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_feqd_not_equal() {
    let asm = r#"
.text
main:
    feq.d t0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);
    set_freg(&mut m, "f2", 4.0);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_fltd_less() {
    let asm = r#"
.text
main:
    flt.d t0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.0);
    set_freg(&mut m, "f2", 3.0);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_fltd_not_less() {
    let asm = r#"
.text
main:
    flt.d t0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);
    set_freg(&mut m, "f2", 2.0);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_fled_less_or_equal() {
    let asm = r#"
.text
main:
    fle.d t0, f1, f2
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.0);
    set_freg(&mut m, "f2", 3.0);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

// ============================================================
// Float load/store tests
// ============================================================

#[test]
fn test_fld() {
    let asm = r#"
.text
main:
    fld f0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    // Write a known double value to memory at address 0x1000
    let hart = m.get_default_hart_mut().unwrap();
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    // Use memory directly
    let val = 3.14159265358979f64;
    let bits = val.to_bits();
    m.memory.write_u64(test_addr, bits);
    step(&mut m);
    assert!((get_freg(&m, "f0") - val).abs() < 0.0001);
}

#[test]
fn test_fsd() {
    let asm = r#"
.text
main:
    fsd f0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    set_freg(&mut m, "f0", 3.14159265358979);
    step(&mut m);
    let result_bits = m.memory.read_u64(test_addr);
    let result = f64::from_bits(result_bits);
    assert!((result - 3.14159265358979).abs() < 0.0001);
}

// ============================================================
// Missing float conversion tests
// ============================================================

#[test]
fn test_fcvtwud() {
    let asm = r#"
.text
main:
    fcvt.wu.d t0, f1
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 3.7);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 3);
}

#[test]
fn test_fcvtdwu() {
    let asm = r#"
.text
main:
    fcvt.d.wu f0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5u64);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 5.0).abs() < 0.0001);
}

// ============================================================
// FMA Float32 tests
// ============================================================

#[test]
fn test_fmadds() {
    // fmadd.s rd, rs1, rs2, rs3 => rd = (rs1 * rs2) + rs3
    let asm = r#"
.text
main:
    fmadd.s f0, f1, f2, f3
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.0f32 as f64);
    set_freg(&mut m, "f2", 3.0f32 as f64);
    set_freg(&mut m, "f3", 4.0f32 as f64);
    step(&mut m);
    // (2.0 * 3.0) + 4.0 = 10.0
    assert!((get_freg(&m, "f0") - 10.0).abs() < 0.0001);
}

#[test]
fn test_fmsubs() {
    // fmsub.s rd, rs1, rs2, rs3 => rd = (rs1 * rs2) - rs3
    let asm = r#"
.text
main:
    fmsub.s f0, f1, f2, f3
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.0f32 as f64);
    set_freg(&mut m, "f2", 3.0f32 as f64);
    set_freg(&mut m, "f3", 4.0f32 as f64);
    step(&mut m);
    // (2.0 * 3.0) - 4.0 = 2.0
    assert!((get_freg(&m, "f0") - 2.0).abs() < 0.0001);
}

#[test]
fn test_fnmsubs() {
    // fnmsub.s rd, rs1, rs2, rs3 => rd = -(rs1 * rs2) + rs3
    let asm = r#"
.text
main:
    fnmsub.s f0, f1, f2, f3
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.0f32 as f64);
    set_freg(&mut m, "f2", 3.0f32 as f64);
    set_freg(&mut m, "f3", 4.0f32 as f64);
    step(&mut m);
    // -(2.0 * 3.0) + 4.0 = -2.0
    assert!((get_freg(&m, "f0") + 2.0).abs() < 0.0001);
}

#[test]
fn test_fnmadds() {
    // fnmadd.s rd, rs1, rs2, rs3 => rd = -(rs1 * rs2) - rs3
    let asm = r#"
.text
main:
    fnmadd.s f0, f1, f2, f3
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.0f32 as f64);
    set_freg(&mut m, "f2", 3.0f32 as f64);
    set_freg(&mut m, "f3", 4.0f32 as f64);
    step(&mut m);
    // -(2.0 * 3.0) - 4.0 = -10.0
    assert!((get_freg(&m, "f0") + 10.0).abs() < 0.0001);
}

// ============================================================
// FMA Float64 tests
// ============================================================

#[test]
fn test_fmaddd() {
    // fmadd.d rd, rs1, rs2, rs3 => rd = (rs1 * rs2) + rs3
    let asm = r#"
.text
main:
    fmadd.d f0, f1, f2, f3
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.0);
    set_freg(&mut m, "f2", 3.0);
    set_freg(&mut m, "f3", 4.0);
    step(&mut m);
    // (2.0 * 3.0) + 4.0 = 10.0
    assert!((get_freg(&m, "f0") - 10.0).abs() < 0.0001);
}

#[test]
fn test_fmsubd() {
    // fmsub.d rd, rs1, rs2, rs3 => rd = (rs1 * rs2) - rs3
    let asm = r#"
.text
main:
    fmsub.d f0, f1, f2, f3
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.0);
    set_freg(&mut m, "f2", 3.0);
    set_freg(&mut m, "f3", 4.0);
    step(&mut m);
    // (2.0 * 3.0) - 4.0 = 2.0
    assert!((get_freg(&m, "f0") - 2.0).abs() < 0.0001);
}

#[test]
fn test_fnmsubd() {
    // fnmsub.d rd, rs1, rs2, rs3 => rd = -(rs1 * rs2) + rs3
    let asm = r#"
.text
main:
    fnmsub.d f0, f1, f2, f3
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.0);
    set_freg(&mut m, "f2", 3.0);
    set_freg(&mut m, "f3", 4.0);
    step(&mut m);
    // -(2.0 * 3.0) + 4.0 = -2.0
    assert!((get_freg(&m, "f0") + 2.0).abs() < 0.0001);
}

#[test]
fn test_fnmaddd() {
    // fnmadd.d rd, rs1, rs2, rs3 => rd = -(rs1 * rs2) - rs3
    let asm = r#"
.text
main:
    fnmadd.d f0, f1, f2, f3
"#;
    let mut m = setup_machine(asm);
    set_freg(&mut m, "f1", 2.0);
    set_freg(&mut m, "f2", 3.0);
    set_freg(&mut m, "f3", 4.0);
    step(&mut m);
    // -(2.0 * 3.0) - 4.0 = -10.0
    assert!((get_freg(&m, "f0") + 10.0).abs() < 0.0001);
}

// ============================================================
// Float Load/Store 32-bit tests
// ============================================================

#[test]
fn test_flw() {
    // flw rd, offset(rs1) => load 32-bit float from memory
    let asm = r#"
.text
main:
    flw f0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    let val = 3.14f32;
    let bits = val.to_bits();
    m.memory.write_u32(test_addr, bits);
    step(&mut m);
    assert!((get_freg(&m, "f0") - 3.14).abs() < 0.001);
}

#[test]
fn test_fsw() {
    // fsw rs2, offset(rs1) => store 32-bit float to memory
    let asm = r#"
.text
main:
    fsw f0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    set_freg(&mut m, "f0", 3.14f32 as f64);
    step(&mut m);
    let result_bits = m.memory.read_u32(test_addr);
    let result = f32::from_bits(result_bits);
    assert!((result - 3.14f32).abs() < 0.001);
}

// ============================================================
// Krypto: Zbkb - Pack/Packh/Packw tests
// ============================================================

#[test]
fn test_pack() {
    // Zbkb pack: X(rd) = {X(rs2)[31:0], X(rs1)[31:0]}
    // rs1 = 0x12345678ABCDEF00, rs1[31:0] = 0xABCDEF00
    // rs2 = 0xAABBCCDDEEFF0011, rs2[31:0] = 0xEEFF0011
    let asm = r#"
.text
main:
    pack t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x12345678ABCDEF00u64);
    set_reg(&mut m, "t2", 0xAABBCCDDEEFF0011u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xEEFF0011ABCDEF00u64);
}

#[test]
fn test_packh() {
    // Zbkb packh: interleave low bytes of rs1 and rs2 into halfwords
    // rs1 = 0x0123456789ABCDEF (little-endian: byte0=0xEF, byte1=0xCD, ...)
    // rs2 = 0x1020304050607080 (little-endian: byte0=0x80, byte1=0x70, ...)
    // result[15:0]   = rs2[7:0]<<8   | rs1[7:0]   = 0x80EF
    // result[31:16]  = rs2[15:8]<<8  | rs1[15:8]  = 0x70CD
    // result[47:32]  = rs2[23:16]<<8 | rs1[23:16] = 0x60AB
    // result[63:48]  = rs2[31:24]<<8 | rs1[31:24] = 0x5089
    let asm = r#"
.text
main:
    packh t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0123456789ABCDEFu64);
    set_reg(&mut m, "t2", 0x1020304050607080u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x508960AB70CD80EFu64);
}

#[test]
fn test_packw() {
    let asm = r#"
.text
main:
    packw t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xABCDu64);
    set_reg(&mut m, "t2", 0x1234u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x1234ABCDu64);
}

// ============================================================
// Krypto: Zbkb - Brev8/Zip/Unzip tests
// ============================================================

#[test]
fn test_brev8() {
    let asm = r#"
.text
main:
    brev8 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0102030405060708u64);
    step(&mut m);
    // Each byte bit-reversed in place (Zbkb brev8):
    // byte 7: 0x01 -> 0x80, byte 6: 0x02 -> 0x40, byte 5: 0x03 -> 0xC0, byte 4: 0x04 -> 0x20,
    // byte 3: 0x05 -> 0xA0, byte 2: 0x06 -> 0x60, byte 1: 0x07 -> 0xE0, byte 0: 0x08 -> 0x10
    assert_eq!(get_reg(&m, "t0"), 0x8040C020A060E010u64);
}

#[test]
fn test_zip() {
    let asm = r#"
.text
main:
    zip t0, t1
"#;
    let mut m = setup_machine(asm);
    // 0xFFFF0000_0000FFFF: low32=0x0000FFFF, high32=0xFFFF0000
    // interleave: bits from low go to even positions, bits from high go to odd
    set_reg(&mut m, "t1", 0xFFFF00000000FFFFu64);
    step(&mut m);
    // After zip: each bit from low32 goes to even positions, from high32 to odd
    // low32=0x0000FFFF -> bits 0-15 set in even positions -> 0x5555
    // high32=0xFFFF0000 -> bits 48-63 set in odd positions -> 0xAAAA...
    // Result: 0xAAAAAAAA55555555
    assert_eq!(get_reg(&m, "t0"), 0xAAAAAAAA55555555u64);
}

#[test]
fn test_unzip() {
    let asm = r#"
.text
main:
    unzip t0, t1
"#;
    let mut m = setup_machine(asm);
    // Input: 0xAAAAAAAA55555555
    // Even bits (0,2,4,...) go to low32: 0x55555555 -> 0x0000FFFF
    // Odd bits (1,3,5,...) go to high32: 0xAAAAAAAA -> 0xFFFF0000
    set_reg(&mut m, "t1", 0xAAAAAAAA55555555u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xFFFF00000000FFFFu64);
}

// ============================================================
// Krypto: Zbkx - Xperm4/Xperm8 tests
// ============================================================

#[test]
fn test_xperm4() {
    // Zbkx xperm4: permute nibbles of rs2 based on indices in rs1
    // rs1 = 0x321: nibble 0=0x1, nibble 1=0x2, nibble 2=0x3, nibble 3..15=0x0
    // rs2 = 0xDCBA: nibble 0=0xA, nibble 1=0xB, nibble 2=0xC, nibble 3=0xD
    // result nibble 0: idx=1 -> 0xB
    // result nibble 1: idx=2 -> 0xC
    // result nibble 2: idx=3 -> 0xD
    // result nibble 3..15: idx=0 -> 0xA
    let asm = r#"
.text
main:
    xperm4 t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x321u64);
    set_reg(&mut m, "t2", 0xDCBAu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xAAAAAAAAAAAAADCBu64);
}

#[test]
fn test_xperm8() {
    // Zbkx xperm8: permute bytes of rs2 based on indices in rs1 (idx >= 8 -> 0)
    // rs1 = 0x4030201: byte 0=0x01, byte 1=0x02, byte 2=0x03, byte 3=0x04, rest 0
    // rs2 = 0x0807060504030201: byte 0=0x01, byte 1=0x02, ..., byte 7=0x08
    // result byte 0: idx=1 -> rs2 byte 1 = 0x02
    // result byte 1: idx=2 -> rs2 byte 2 = 0x03
    // result byte 2: idx=3 -> rs2 byte 3 = 0x04
    // result byte 3: idx=4 -> rs2 byte 4 = 0x05
    // result byte 4-7: idx=0 -> rs2 byte 0 = 0x01
    let asm = r#"
.text
main:
    xperm8 t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x4030201u64);
    set_reg(&mut m, "t2", 0x0807060504030201u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0101010105040302u64);
}

// ============================================================
// Krypto: AES tests (with real AES S-Box/MixColumns)
// ============================================================

#[test]
fn test_aes64es_zero() {
    // aes64es: forward S-Box on each byte, then MixColumns on each 32-bit half, XOR rs2
    // SBox[0x00] = 0x63 for all bytes -> 0x63636363_63636363
    // MixColumns on 0x63636363 = 0x63636363 (all bytes equal)
    // XOR with rs2=0 -> 0x63636363_63636363
    let asm = r#"
.text
main:
    aes64es t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0000000000000000u64);
    set_reg(&mut m, "t2", 0x0000000000000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x6363636363636363u64);
}

#[test]
fn test_aes64es_mixed() {
    // aes64es: SubBytes + MixColumns on each 32-bit half, XOR rs2=0
    // Input: 0x6745230167452301
    // Each byte SubBytes then MixColumns on each 32-bit half
    let asm = r#"
.text
main:
    aes64es t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x6745230167452301u64);
    set_reg(&mut m, "t2", 0x0000000000000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xDD120779DD120779u64);
}

#[test]
fn test_aes64es_with_rs2() {
    // aes64es: SubBytes + MixColumns on rs1, then XOR with rs2
    // Using zero rs1 for simplicity: SubBytes=0x63636363_63636363, MixColumns=0x63636363_63636363
    // XOR with rs2=0xFFFFFFFF_FFFFFFFF = 0x9C9C9C9C_9C9C9C9C
    let asm = r#"
.text
main:
    aes64es t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0000000000000000u64);
    set_reg(&mut m, "t2", 0xFFFFFFFFFFFFFFFFu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x9C9C9C9C9C9C9C9Cu64);
}

#[test]
fn test_aes64ds_zero() {
    // aes64ds: inverse S-Box on each byte, then inverse MixColumns on each 32-bit half, XOR rs2
    // InvSBox[0x00] = 0x52 for all bytes -> 0x52525252_52525252
    // InvMixColumns on 0x52525252 = 0x52525252 (all bytes equal)
    // XOR with rs2=0 -> 0x52525252_52525252
    let asm = r#"
.text
main:
    aes64ds t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0000000000000000u64);
    set_reg(&mut m, "t2", 0x0000000000000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x5252525252525252u64);
}

#[test]
fn test_aes64esm_zero() {
    // aes64esm: MixColumns on each 32-bit half of rs1, XOR with rs2
    // MixColumns on 0x00000000 = 0x00000000, XOR 0 = 0
    let asm = r#"
.text
main:
    aes64esm t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0000000000000000u64);
    set_reg(&mut m, "t2", 0x0000000000000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0000000000000000u64);
}

#[test]
fn test_aes64esm_mixed() {
    // aes64esm: MixColumns on each 32-bit half, XOR rs2=0
    // Input: 0x0100000001000000 (byte3=0x01, rest=0 in each 32-bit half)
    // MixColumns: b0=0x00,b1=0x00,b2=0x00,b3=0x01 -> 0x02030101 per half
    let asm = r#"
.text
main:
    aes64esm t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0100000001000000u64);
    set_reg(&mut m, "t2", 0x0000000000000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0203010102030101u64);
}

#[test]
fn test_aes64dsm_zero() {
    // aes64dsm: inverse MixColumns on each 32-bit half, XOR with rs2
    // InvMixColumns on 0x00000000 = 0x00000000, XOR 0 = 0
    let asm = r#"
.text
main:
    aes64dsm t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0000000000000000u64);
    set_reg(&mut m, "t2", 0x0000000000000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0000000000000000u64);
}

#[test]
fn test_aes64dsm_mixed() {
    // aes64dsm: InvMixColumns on each 32-bit half, XOR rs2=0
    // Input: 0x0100000001000000 (byte3=0x01, rest=0 in each 32-bit half)
    // InvMixColumns: b0=0x00,b1=0x00,b2=0x00,b3=0x01 -> 0x0E0B0D09 per half
    let asm = r#"
.text
main:
    aes64dsm t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0100000001000000u64);
    set_reg(&mut m, "t2", 0x0000000000000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0E0B0D090E0B0D09u64);
}

#[test]
fn test_aes64im_zero() {
    // aes64im: inverse MixColumns on 64-bit value (treat as 2x2 state of 32-bit columns)
    // All zeros -> InvMixColumns on 0x00000000 = 0
    let asm = r#"
.text
main:
    aes64im t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0000000000000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0000000000000000u64);
}

#[test]
fn test_aes64im_nonzero() {
    // aes64im: InvMixColumns on each 32-bit half (same as aes64dsm without rs2 XOR)
    // Input: 0x0100000001000000 -> InvMixColumns per half = 0x0E0B0D09
    let asm = r#"
.text
main:
    aes64im t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0100000001000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0E0B0D090E0B0D09u64);
}

#[test]
fn test_aes64ks1i_rcon1() {
    // aes64ks1i: SubWord(RotWord(w1)) ^ rcon, w1=0, rcon=1
    // SubWord(0) = 0x63636363, XOR rcon in MSB: 0x62_636363
    let asm = r#"
.text
main:
    aes64ks1i t0, t1, 1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0000000000000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x62636363u64);
}

#[test]
fn test_aes64ks1i_rcon5() {
    // aes64ks1i with rcon=5: SubWord(0)=0x63636363, XOR rcon=5 in MSB -> 0x66_636363
    let asm = r#"
.text
main:
    aes64ks1i t0, t1, 5
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0000000000000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x66636363u64);
}

#[test]
fn test_aes64ks2() {
    // aes64ks2: XOR rs1 with rs2 (two round key words)
    let asm = r#"
.text
main:
    aes64ks2 t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x0123456789ABCDEFu64);
    set_reg(&mut m, "t2", 0xFEDCBA9876543210u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xFFFFFFFFFFFFFFFFu64);
}

// ============================================================
// Krypto: SHA256 tests
// ============================================================

#[test]
fn test_sha256sig0() {
    let asm = r#"
.text
main:
    sha256sig0 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x1234567890ABCDEFu64);
    step(&mut m);
    let val = 0x1234567890ABCDEFu64;
    let expected = val.rotate_right(7) ^ val.rotate_right(18) ^ (val >> 3);
    assert_eq!(get_reg(&m, "t0"), expected);
}

#[test]
fn test_sha256sig1() {
    let asm = r#"
.text
main:
    sha256sig1 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x1234567890ABCDEFu64);
    step(&mut m);
    let val = 0x1234567890ABCDEFu64;
    let expected = val.rotate_right(17) ^ val.rotate_right(19) ^ (val >> 10);
    assert_eq!(get_reg(&m, "t0"), expected);
}

#[test]
fn test_sha256sum0() {
    let asm = r#"
.text
main:
    sha256sum0 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x1234567890ABCDEFu64);
    step(&mut m);
    let val = 0x1234567890ABCDEFu64;
    let expected = val.rotate_right(2) ^ val.rotate_right(13) ^ val.rotate_right(22);
    assert_eq!(get_reg(&m, "t0"), expected);
}

#[test]
fn test_sha256sum1() {
    let asm = r#"
.text
main:
    sha256sum1 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x1234567890ABCDEFu64);
    step(&mut m);
    let val = 0x1234567890ABCDEFu64;
    let expected = val.rotate_right(6) ^ val.rotate_right(11) ^ val.rotate_right(25);
    assert_eq!(get_reg(&m, "t0"), expected);
}

// ============================================================
// Krypto: SHA512 tests
// ============================================================

#[test]
fn test_sha512sig0() {
    let asm = r#"
.text
main:
    sha512sig0 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x1234567890ABCDEFu64);
    step(&mut m);
    let val = 0x1234567890ABCDEFu64;
    let expected = val.rotate_right(1) ^ val.rotate_right(8) ^ (val >> 7);
    assert_eq!(get_reg(&m, "t0"), expected);
}

#[test]
fn test_sha512sig1() {
    let asm = r#"
.text
main:
    sha512sig1 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x1234567890ABCDEFu64);
    step(&mut m);
    let val = 0x1234567890ABCDEFu64;
    let expected = val.rotate_right(19) ^ val.rotate_right(61) ^ (val >> 6);
    assert_eq!(get_reg(&m, "t0"), expected);
}

#[test]
fn test_sha512sum0() {
    let asm = r#"
.text
main:
    sha512sum0 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x1234567890ABCDEFu64);
    step(&mut m);
    let val = 0x1234567890ABCDEFu64;
    let expected = val.rotate_right(28) ^ val.rotate_right(34) ^ val.rotate_right(39);
    assert_eq!(get_reg(&m, "t0"), expected);
}

#[test]
fn test_sha512sum1() {
    let asm = r#"
.text
main:
    sha512sum1 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x1234567890ABCDEFu64);
    step(&mut m);
    let val = 0x1234567890ABCDEFu64;
    let expected = val.rotate_right(14) ^ val.rotate_right(18) ^ val.rotate_right(41);
    assert_eq!(get_reg(&m, "t0"), expected);
}

// ============================================================
// Krypto: SM3 tests
// ============================================================

#[test]
fn test_sm3p0() {
    let asm = r#"
.text
main:
    sm3p0 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x1234567890ABCDEFu64);
    step(&mut m);
    let val = 0x1234567890ABCDEFu64;
    let expected = val ^ val.rotate_right(9) ^ val.rotate_right(17);
    assert_eq!(get_reg(&m, "t0"), expected);
}

#[test]
fn test_sm3p1() {
    let asm = r#"
.text
main:
    sm3p1 t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x1234567890ABCDEFu64);
    step(&mut m);
    let val = 0x1234567890ABCDEFu64;
    let expected = val ^ val.rotate_right(15) ^ val.rotate_right(23);
    assert_eq!(get_reg(&m, "t0"), expected);
}
