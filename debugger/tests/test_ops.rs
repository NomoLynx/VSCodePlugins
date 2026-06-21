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