#[path = "../src/utility.rs"]
mod utility;
use crate::utility::*;

// Sub-module declarations at crate root (required by `crate::machine::...` etc.)
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

// ============================================================
// RV32I: Basic integer arithmetic tests (R-type)
// ============================================================

#[test]
fn test_add() {
    let asm = r#"
.text
main:
    add t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10);
    set_reg(&mut m, "t2", 20);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 30);
}

#[test]
fn test_sub() {
    let asm = r#"
.text
main:
    sub t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 50);
    set_reg(&mut m, "t2", 20);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 30);
}

#[test]
fn test_and() {
    let asm = r#"
.text
main:
    and t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFu64);
    set_reg(&mut m, "t2", 0x0Fu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0F);
}

#[test]
fn test_or() {
    let asm = r#"
.text
main:
    or t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xF0u64);
    set_reg(&mut m, "t2", 0x0Fu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xFF);
}

#[test]
fn test_xor() {
    let asm = r#"
.text
main:
    xor t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFu64);
    set_reg(&mut m, "t2", 0xF0u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0F);
}

#[test]
fn test_sll() {
    let asm = r#"
.text
main:
    sll t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    set_reg(&mut m, "t2", 3);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 40); // 5 << 3 = 40
}

#[test]
fn test_srl() {
    let asm = r#"
.text
main:
    srl t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFF00000000u64);
    set_reg(&mut m, "t2", 4);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0FFFFFFFF0000000);
}

#[test]
fn test_sra() {
    let asm = r#"
.text
main:
    sra t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFF00000000u64);
    set_reg(&mut m, "t2", 4);
    step(&mut m);
    // arithmetic shift: sign bit extends
    assert_eq!(get_reg(&m, "t0"), 0xFFFFFFFFF0000000u64);
}

#[test]
fn test_slt_true() {
    let asm = r#"
.text
main:
    slt t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (-5i64) as u64);
    set_reg(&mut m, "t2", 10);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_slt_false() {
    let asm = r#"
.text
main:
    slt t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10);
    set_reg(&mut m, "t2", (-5i64) as u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_sltu() {
    let asm = r#"
.text
main:
    sltu t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    set_reg(&mut m, "t2", 10);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_sltu_false() {
    let asm = r#"
.text
main:
    sltu t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10);
    set_reg(&mut m, "t2", 5);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

// ============================================================
// RV32I: Immediate arithmetic tests (I-type)
// ============================================================

#[test]
fn test_addi() {
    let asm = r#"
.text
main:
    addi t0, t1, 42
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 52);
}

#[test]
fn test_addi_negative() {
    let asm = r#"
.text
main:
    addi t0, t1, -5
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 5);
}

#[test]
fn test_andi() {
    let asm = r#"
.text
main:
    andi t0, t1, 0xFF
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xABCDu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xCD);
}

#[test]
fn test_ori() {
    let asm = r#"
.text
main:
    ori t0, t1, 0x0F
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xF0u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xFF);
}

#[test]
fn test_xori() {
    let asm = r#"
.text
main:
    xori t0, t1, 0xFF
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFu64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_slti_true() {
    let asm = r#"
.text
main:
    slti t0, t1, 5
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", (-10i64) as u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_slti_false() {
    let asm = r#"
.text
main:
    slti t0, t1, -5
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_sltiu_true() {
    let asm = r#"
.text
main:
    sltiu t0, t1, 10
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_sltiu_false() {
    let asm = r#"
.text
main:
    sltiu t0, t1, 5
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_slli() {
    let asm = r#"
.text
main:
    slli t0, t1, 3
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 40);
}

#[test]
fn test_srli() {
    let asm = r#"
.text
main:
    srli t0, t1, 4
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFF00000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0FFFFFFFF0000000);
}

#[test]
fn test_srai() {
    let asm = r#"
.text
main:
    srai t0, t1, 4
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0xFFFFFFFF00000000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xFFFFFFFFF0000000u64);
}

#[test]
fn test_srai_positive() {
    let asm = r#"
.text
main:
    srai t0, t1, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0x8000000000000000u64); // i64::MIN
    step(&mut m);
    // -2^63 >> 2 = -2^61 = 0xE000000000000000
    assert_eq!(get_reg(&m, "t0"), 0xE000000000000000u64);
}

// ============================================================
// RV32I: Load instructions
// ============================================================

#[test]
fn test_lb() {
    let asm = r#"
.text
main:
    lb t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u8(test_addr, 0xFF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), u64::MAX); // sign-extended to -1
}

#[test]
fn test_lb_positive() {
    let asm = r#"
.text
main:
    lb t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u8(test_addr, 0x7F);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 127);
}

#[test]
fn test_lbu() {
    let asm = r#"
.text
main:
    lbu t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u8(test_addr, 0xFF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 255);
}

#[test]
fn test_lh() {
    let asm = r#"
.text
main:
    lh t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u16(test_addr, 0xFFFF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), u64::MAX);
}

#[test]
fn test_lh_positive() {
    let asm = r#"
.text
main:
    lh t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u16(test_addr, 0x7FFF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 32767);
}

#[test]
fn test_lhu() {
    let asm = r#"
.text
main:
    lhu t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u16(test_addr, 0xFFFF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 65535);
}

#[test]
fn test_lw() {
    let asm = r#"
.text
main:
    lw t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u32(test_addr, 0xFFFFFFFF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), u64::MAX);
}

#[test]
fn test_lw_positive() {
    let asm = r#"
.text
main:
    lw t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u32(test_addr, 0x7FFFFFFF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 2147483647);
}

#[test]
fn test_lwu() {
    let asm = r#"
.text
main:
    lwu t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u32(test_addr, 0xFFFFFFFF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0xFFFFFFFF);
}

#[test]
fn test_ld() {
    let asm = r#"
.text
main:
    ld t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u64(test_addr, 0x0123456789ABCDEF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0123456789ABCDEF);
}

#[test]
fn test_lb_with_offset() {
    let asm = r#"
.text
main:
    lb t0, 4(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u8(test_addr + 4, 0x42);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x42);
}

// ============================================================
// RV32I: Store instructions
// ============================================================

#[test]
fn test_sb() {
    let asm = r#"
.text
main:
    sb t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    set_reg(&mut m, "t0", 0xAB);
    step(&mut m);
    assert_eq!(m.memory.read_u8(test_addr), 0xAB);
}

#[test]
fn test_sh() {
    let asm = r#"
.text
main:
    sh t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    set_reg(&mut m, "t0", 0xABCD);
    step(&mut m);
    assert_eq!(m.memory.read_u16(test_addr), 0xABCD);
}

#[test]
fn test_sw() {
    let asm = r#"
.text
main:
    sw t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    set_reg(&mut m, "t0", 0xDEADBEEF);
    step(&mut m);
    assert_eq!(m.memory.read_u32(test_addr), 0xDEADBEEF);
}

#[test]
fn test_sd() {
    let asm = r#"
.text
main:
    sd t0, 0(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    set_reg(&mut m, "t0", 0x0123456789ABCDEF);
    step(&mut m);
    assert_eq!(m.memory.read_u64(test_addr), 0x0123456789ABCDEF);
}

#[test]
fn test_sb_with_offset() {
    let asm = r#"
.text
main:
    sb t0, 8(t1)
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    set_reg(&mut m, "t0", 0x42);
    step(&mut m);
    assert_eq!(m.memory.read_u8(test_addr + 8), 0x42);
}

// ============================================================
// RV32I: Lui and Auipc
// ============================================================

#[test]
fn test_lui() {
    let asm = r#"
.text
main:
    lui t0, 0x12345
"#;
    let mut m = setup_machine(asm);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x12345000);
}

#[test]
fn test_auipc() {
    let asm = r#"
.text
main:
    auipc t0, 0x10
"#;
    let mut m = setup_machine(asm);
    let pc = m.get_default_hart().unwrap().pc as u64;
    step(&mut m);
    let expected = pc.wrapping_add_signed(0x10 << 12);
    assert_eq!(get_reg(&m, "t0"), expected);
}

// ============================================================
// RV32I: Branch instructions
// Note: The machine uses u64 comparison for all branches.
// Blt/Bge compare u64 (not i64), same as Bltu/Bgeu.
// ============================================================

#[test]
fn test_beq_taken() {
    let asm = r#"
.text
main:
    beq t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    set_reg(&mut m, "t2", 5);
    step(&mut m); // beq: taken, skip addi t0, x0, 1
    step(&mut m); // addi t0, x0, 2
    assert_eq!(get_reg(&m, "t0"), 2);
}

#[test]
fn test_beq_not_taken() {
    let asm = r#"
.text
main:
    beq t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    set_reg(&mut m, "t2", 10);
    step(&mut m); // beq: not taken
    step(&mut m); // addi t0, x0, 1
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_bne_taken() {
    let asm = r#"
.text
main:
    bne t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    set_reg(&mut m, "t2", 10);
    step(&mut m); // bne: taken
    step(&mut m); // addi t0, x0, 2
    assert_eq!(get_reg(&m, "t0"), 2);
}

#[test]
fn test_bne_not_taken() {
    let asm = r#"
.text
main:
    bne t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    set_reg(&mut m, "t2", 5);
    step(&mut m); // bne: not taken
    step(&mut m); // addi t0, x0, 1
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_blt_taken() {
    let asm = r#"
.text
main:
    blt t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    set_reg(&mut m, "t2", 10);
    step(&mut m); // blt: 5 < 10, taken
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 2);
}

#[test]
fn test_blt_not_taken() {
    let asm = r#"
.text
main:
    blt t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10);
    set_reg(&mut m, "t2", 5);
    step(&mut m); // blt: 10 < 5, not taken
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_bge_taken() {
    let asm = r#"
.text
main:
    bge t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10);
    set_reg(&mut m, "t2", 5);
    step(&mut m); // bge: 10 >= 5, taken
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 2);
}

#[test]
fn test_bge_not_taken() {
    let asm = r#"
.text
main:
    bge t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    set_reg(&mut m, "t2", 10);
    step(&mut m); // bge: 5 >= 10, not taken
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_bltu_taken() {
    let asm = r#"
.text
main:
    bltu t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    set_reg(&mut m, "t2", 10);
    step(&mut m); // bltu: taken
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 2);
}

#[test]
fn test_bltu_not_taken() {
    let asm = r#"
.text
main:
    bltu t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10);
    set_reg(&mut m, "t2", 5);
    step(&mut m); // bltu: not taken
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_bgeu_taken() {
    let asm = r#"
.text
main:
    bgeu t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 10);
    set_reg(&mut m, "t2", 5);
    step(&mut m); // bgeu: taken
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 2);
}

#[test]
fn test_bgeu_not_taken() {
    let asm = r#"
.text
main:
    bgeu t1, t2, 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5);
    set_reg(&mut m, "t2", 10);
    step(&mut m); // bgeu: not taken
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 1);
}

// ============================================================
// RV32I: Jump instructions
// ============================================================

#[test]
fn test_jal() {
    // jal pseudoinstruction: jal offset -> sets ra and jumps
    // PEG: pseudoinstructions_offset_incs ~ offset
    let asm = r#"
.text
main:
    jal 8
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    step(&mut m); // jal: jumps to offset 8, ra = return address
    step(&mut m); // addi t0, x0, 2 (skipped addi t0, x0, 1)
    assert_eq!(get_reg(&m, "t0"), 2);
    assert!(get_reg(&m, "ra") > 0);
}

#[test]
fn test_jalr() {
    let asm = r#"
.text
main:
    addi t1, x0, 108
    jalr ra, 4(t1)
    addi t0, x0, 1
    addi t0, x0, 2
"#;
    let mut m = setup_machine(asm);
    step(&mut m); // addi t1, x0, 108
    step(&mut m); // jalr: jumps to 108+4=112, ra = return address
    step(&mut m); // addi t0, x0, 2 (skipped the first one)
    assert_eq!(get_reg(&m, "t0"), 2);
}

// ============================================================
// C Extension: Basic arithmetic
// NOTE: Implementation uses PEG grammar where:
//   rd, imm format: r0=rd, r1=None (→x0=0). So rs1 is always x0.
//   rd, rs1 format: r0=rd, r1=rs1, r2=None (→x0=0). So rs2 is always x0.
// ============================================================

#[test]
fn test_caddi() {
    // c.addi t0, 5: r0=t0, r1=None→x0=0. So t0 = x0 + 5 = 5.
    let asm = r#"
.text
main:
    c.addi t0, 5
"#;
    let mut m = setup_machine(asm);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 5);
}

#[test]
fn test_caddi_negative() {
    // c.addi t0, -3: r0=t0, r1=None→x0=0. So t0 = 0 + (-3) = u64::MAX - 2.
    let asm = r#"
.text
main:
    c.addi t0, -3
"#;
    let mut m = setup_machine(asm);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, -3);
}

#[test]
fn test_cmv() {
    // c.mv t0, t1: PEG: rd, rs1 → r0=t0, r1=t1. t0 = t1 = 42.
    let asm = r#"
.text
main:
    c.mv t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 42);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 42);
}

#[test]
fn test_cli() {
    // c.addi t0, 42: r0=t0, r1=None→x0=0. t0 = 0 + 42 = 42.
    let asm = r#"
.text
main:
    c.addi t0, 42
"#;
    let mut m = setup_machine(asm);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 42);
}

#[test]
fn test_cli_negative() {
    let asm = r#"
.text
main:
    c.addi t0, -1
"#;
    let mut m = setup_machine(asm);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0") as i64, -1);
}

#[test]
fn test_clui() {
    // c.lui t0, 0x1F: imm << 12 = 0x1F000
    let asm = r#"
.text
main:
    c.lui t0, 0x1F
"#;
    let mut m = setup_machine(asm);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x1F000);
}

#[test]
fn test_cadd() {
    // c.add t0, t1: t0 = t0 + t1. PEG: r0=rd, r1=rs2.
    let asm = r#"
.text
main:
    c.add t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t0", 10);
    set_reg(&mut m, "t1", 20);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 30);
}

#[test]
fn test_csub() {
    // c.sub t0, t1: t0 = t0 - t1. PEG: r0=rd, r1=rs2.
    let asm = r#"
.text
main:
    c.sub t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t0", 50);
    set_reg(&mut m, "t1", 20);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 30);
}

#[test]
fn test_cslli() {
    // c.slli t0, 3: t0 = t0 << 3. PEG: r0=rd, r1=None→x0.
    let asm = r#"
.text
main:
    c.slli t0, 3
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t0", 5);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 40); // 5 << 3 = 40
}

#[test]
fn test_caddiw() {
    // c.addiw t0, 5: t0 = (t0 as i32).wrapping_add(5) sign-extended. PEG: r0=rd, r1=None→x0.
    let asm = r#"
.text
main:
    c.addiw t0, 5
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t0", 10);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 15);
}

#[test]
fn test_caddw() {
    // c.addw t0, t1: t0 = (t0 as i32 + t1 as i32) sign-extended. PEG: r0=rd, r1=rs2.
    let asm = r#"
.text
main:
    c.addw t0, t1
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t0", 5);
    set_reg(&mut m, "t1", 3);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 8);
}

// ============================================================
// C Extension: Branches and jumps
// Syntax: c.beqz rs1, imm   (branch if rs1 == 0)
//         c.bnez rs1, imm   (branch if rs1 != 0)
//         c.j imm           (jump)
//         c.jr rs1          (jump register)
//         c.jal imm         (jump and link, RV32 only)
//         c.jalr rs1        (jump and link register)
// NOTE: C instructions are 2 bytes. When mixing with 4-byte RV32I,
// branch offset must align to valid instruction boundaries.
// c.beqz at offset 100 (2 bytes), then c.addi at 102 (2 bytes),
// c.addi at 104 (2 bytes). Branch offset 2 skips one C inst.
// ============================================================

#[test]
fn test_cbeqz_taken() {
    // c.beqz rs1, imm: branch if rs1 == 0. PEG: r0=rs1, r1=None.
    // t1=0 → branch taken, skip one C instruction.
    let asm = r#"
.text
main:
    c.beqz t1, 4
    c.addi t0, 1
    c.addi t0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0); // t1 == 0 → branch taken
    step(&mut m); // c.beqz: taken, pc+=4
    step(&mut m); // c.addi t0, 2 → t0 = 0 + 2 = 2
    assert_eq!(get_reg(&m, "t0"), 2);
}

#[test]
fn test_cbeqz_not_taken() {
    // c.beqz rs1, imm: branch if rs1 == 0.
    // t1=5 (non-zero) → branch NOT taken.
    let asm = r#"
.text
main:
    c.beqz t1, 4
    c.addi t0, 1
    c.addi t0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5); // t1 != 0 → not taken
    step(&mut m); // c.beqz: not taken
    step(&mut m); // c.addi t0, 1 → t0 = 0 + 1 = 1
    assert_eq!(get_reg(&m, "t0"), 1);
}

#[test]
fn test_cbnez_taken() {
    // c.bnez rs1, imm: branch if rs1 != 0. PEG: r0=rs1, r1=None.
    // t1=5 (non-zero) → branch taken.
    let asm = r#"
.text
main:
    c.bnez t1, 4
    c.addi t0, 1
    c.addi t0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 5); // t1 != 0 → branch taken
    step(&mut m); // c.bnez: taken, pc+=4
    step(&mut m); // c.addi t0, 2 → t0 = 0 + 2 = 2
    assert_eq!(get_reg(&m, "t0"), 2);
}

#[test]
fn test_cbnez_not_taken() {
    // c.bnez rs1, imm: branch if rs1 != 0.
    // t1=0 → branch NOT taken.
    let asm = r#"
.text
main:
    c.bnez t1, 4
    c.addi t0, 1
    c.addi t0, 2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0); // t1 == 0 → not taken
    step(&mut m); // c.bnez: not taken
    step(&mut m); // c.addi t0, 1 → t0 = 0 + 1 = 1
    assert_eq!(get_reg(&m, "t0"), 1);
}

// ============================================================
// C Extension: Load/Store
// PEG mappings:
// - C loads (c.lw/c.ld):  rd, rs1, imm → r0=rd, r1=rs1(base)
// - C stores (c.sw/c.sd): rs1, rs2, imm → r0=rs1(base), r1=rs2(value)
// - C stack ops (lwsp/ldsp/swsp/sdsp): rd/rs2, imm, base=sp(x2)
// ============================================================

#[test]
fn test_clw() {
    // c.lw rd, rs1, imm: r0=rd, r1=rs1(base)
    let asm = r#"
.text
main:
    c.lw t0, t1, 0
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u32(test_addr, 0x7FFFFFFF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 2147483647);
}

#[test]
fn test_csw() {
    // PEG: c.sw rs1, rs2, imm → r0=rs1(base), r1=rs2(value)
    // c.sw t0, t1, 0 → r0=t0 (base), r1=t1 (value to store)
    // So set t0=base, t1=value
    let asm = r#"
.text
main:
    c.sw t0, t1, 0
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t0", test_addr);   // r0=base
    set_reg(&mut m, "t1", 0xDEADBEEF);  // r1=value
    step(&mut m);
    assert_eq!(m.memory.read_u32(test_addr), 0xDEADBEEF);
}

#[test]
fn test_cld() {
    // c.ld rd, rs1, imm: r0=rd, r1=rs1(base)
    let asm = r#"
.text
main:
    c.ld t0, t1, 0
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t1", test_addr);
    m.memory.write_u64(test_addr, 0x0123456789ABCDEF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0123456789ABCDEF);
}

#[test]
fn test_csd() {
    // PEG: c.sd rs1, rs2, imm → r0=rs1(base), r1=rs2(value)
    // c.sd t0, t1, 0 → r0=t0 (base), r1=t1 (value to store)
    // So set t0=base, t1=value
    let asm = r#"
.text
main:
    c.sd t0, t1, 0
"#;
    let mut m = setup_machine(asm);
    let test_addr: u64 = 0x1000;
    set_reg(&mut m, "t0", test_addr);                 // r0=base
    set_reg(&mut m, "t1", 0x0123456789ABCDEF);        // r1=value
    step(&mut m);
    assert_eq!(m.memory.read_u64(test_addr), 0x0123456789ABCDEF);
}

#[test]
fn test_clwsp() {
    // c.lwsp rd, imm: loads from sp + imm. Set sp=0x1000, imm=4 → addr=0x1004.
    let asm = r#"
.text
main:
    c.lwsp t0, 4
"#;
    let mut m = setup_machine(asm);
    // Set sp (x2) to 0x1000
    m.get_default_hart_mut().unwrap().x.regs[2].value = 0x1000;
    m.memory.write_u32(0x1004, 0x12345678);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x12345678);
}

#[test]
fn test_cswsp() {
    // c.swsp rs2, imm: stores to sp + imm. Set sp=0x1000, imm=4 → addr=0x1004.
    let asm = r#"
.text
main:
    c.swsp t0, 4
"#;
    let mut m = setup_machine(asm);
    m.get_default_hart_mut().unwrap().x.regs[2].value = 0x1000;
    set_reg(&mut m, "t0", 0xDEADBEEF);
    step(&mut m);
    assert_eq!(m.memory.read_u32(0x1004), 0xDEADBEEF);
}

#[test]
fn test_cldsp() {
    // c.ldsp rd, imm: loads from sp + imm. Set sp=0x1000, imm=8 → addr=0x1008.
    let asm = r#"
.text
main:
    c.ldsp t0, 8
"#;
    let mut m = setup_machine(asm);
    m.get_default_hart_mut().unwrap().x.regs[2].value = 0x1000;
    m.memory.write_u64(0x1008, 0x0123456789ABCDEF);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0x0123456789ABCDEF);
}

#[test]
fn test_csdsp() {
    // c.sdsp rs2, imm: stores to sp + imm. Set sp=0x1000, imm=8 → addr=0x1008.
    let asm = r#"
.text
main:
    c.sdsp t0, 8
"#;
    let mut m = setup_machine(asm);
    m.get_default_hart_mut().unwrap().x.regs[2].value = 0x1000;
    set_reg(&mut m, "t0", 0x0123456789ABCDEF);
    step(&mut m);
    assert_eq!(m.memory.read_u64(0x1008), 0x0123456789ABCDEF);
}
