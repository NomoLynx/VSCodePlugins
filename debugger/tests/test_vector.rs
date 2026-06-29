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
// vsetvl / vsetvli tests
// ============================================================

#[test]
fn test_vsetvli() {
    let asm = r#"
.text
main:
    vsetvli t0, t1, e32,m1,ta,ma
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 4u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 4);
}

#[test]
fn test_vsetvl() {
    let asm = r#"
.text
main:
    vsetvl t0, t1, t2
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 4u64);
    // vtype: e32 (SEW=010), m1 (000) => 0b010_000 = 16
    set_reg(&mut m, "t2", 0x10u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 4);
}

#[test]
fn test_vsetvli_zero_vl() {
    let asm = r#"
.text
main:
    vsetvli t0, t1, e32,m1,ta,ma
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 0u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 0);
}

#[test]
fn test_vsetvli_avl_exceeds_vlmax() {
    // VLEN=512 bits=64 bytes, e8,m1 => VLMAX = 64/1*1 = 64
    // Request AVL=1000, VL should be capped at 64
    let asm = r#"
.text
main:
    vsetvli t0, t1, e8,m1,ta,ma
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 1000u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 64); // capped at VLMAX
    let hart = m.get_default_hart().unwrap();
    assert_eq!(hart.csr.vl, 64);
    // vtype: e8(sew=0), m1(lmul=0), ta=1, ma=1 => 0b1100_0000 = 0xC0
    assert_eq!(hart.csr.vtype, 0xC0);
}

#[test]
fn test_vsetvli_avl_within_vlmax() {
    // VLEN=512 bits=64 bytes, e32,m1 => VLMAX = 64/4*1 = 16
    // Request AVL=4, VL should be 4
    let asm = r#"
.text
main:
    vsetvli t0, t1, e32,m1,ta,ma
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 4u64);
    step(&mut m);
    assert_eq!(get_reg(&m, "t0"), 4);
}

#[test]
fn test_vsetvli_avl_exceeds_vlmax_x0_dest() {
    // When rd=x0, result is not written to any GPR (x0 always 0)
    let asm = r#"
.text
main:
    vsetvli x0, t1, e8,m1,ta,ma
"#;
    let mut m = setup_machine(asm);
    set_reg(&mut m, "t1", 1000u64);
    step(&mut m);
    // x0 should always be 0
    assert_eq!(get_reg(&m, "x0"), 0);
    // VL should still be set to VLMAX
    let hart = m.get_default_hart().unwrap();
    assert_eq!(hart.csr.vl, 64);
}

// ============================================================
// vadd tests
// ============================================================

#[test]
fn test_vadd_vv_e32() {
    let asm = r#"
.text
main:
    vadd.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 1);
    set_velem(&mut v2, 1, 4, 2);
    set_velem(&mut v2, 2, 4, 3);
    set_velem(&mut v2, 3, 4, 4);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 10);
    set_velem(&mut v3, 1, 4, 20);
    set_velem(&mut v3, 2, 4, 30);
    set_velem(&mut v3, 3, 4, 40);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 11);
    assert_eq!(get_velem(&v1, 1, 4), 22);
    assert_eq!(get_velem(&v1, 2, 4), 33);
    assert_eq!(get_velem(&v1, 3, 4), 44);
}

#[test]
fn test_vadd_vx_e32() {
    let asm = r#"
.text
main:
    vadd.vx v1, v2, t0
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 10);
    set_velem(&mut v2, 1, 4, 20);
    set_velem(&mut v2, 2, 4, 30);
    set_velem(&mut v2, 3, 4, 40);
    set_vreg(&mut m, "v2", v2);
    set_reg(&mut m, "t0", 5u64);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 15);
    assert_eq!(get_velem(&v1, 1, 4), 25);
    assert_eq!(get_velem(&v1, 2, 4), 35);
    assert_eq!(get_velem(&v1, 3, 4), 45);
}

#[test]
fn test_vadd_vi_e32() {
    let asm = r#"
.text
main:
    vadd.vi v1, v2, 5
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 10);
    set_velem(&mut v2, 1, 4, 20);
    set_velem(&mut v2, 2, 4, 30);
    set_velem(&mut v2, 3, 4, 40);
    set_vreg(&mut m, "v2", v2);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 15);
    assert_eq!(get_velem(&v1, 1, 4), 25);
    assert_eq!(get_velem(&v1, 2, 4), 35);
    assert_eq!(get_velem(&v1, 3, 4), 45);
}

#[test]
fn test_vadd_vv_e8() {
    let asm = r#"
.text
main:
    vadd.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e8", "m1", 8);
    let mut v2 = vec![0u8; 64];
    for i in 0..8 {
        set_velem(&mut v2, i, 1, (i + 1) as u64);
    }
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    for i in 0..8 {
        set_velem(&mut v3, i, 1, (i * 10) as u64);
    }
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 1), 1);
    assert_eq!(get_velem(&v1, 1, 1), 12);
    assert_eq!(get_velem(&v1, 2, 1), 23);
    assert_eq!(get_velem(&v1, 3, 1), 34);
}

// ============================================================
// vsub tests
// ============================================================

#[test]
fn test_vsub_vv_e32() {
    let asm = r#"
.text
main:
    vsub.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 100);
    set_velem(&mut v2, 1, 4, 200);
    set_velem(&mut v2, 2, 4, 300);
    set_velem(&mut v2, 3, 4, 400);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 10);
    set_velem(&mut v3, 1, 4, 20);
    set_velem(&mut v3, 2, 4, 30);
    set_velem(&mut v3, 3, 4, 40);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 90);
    assert_eq!(get_velem(&v1, 1, 4), 180);
    assert_eq!(get_velem(&v1, 2, 4), 270);
    assert_eq!(get_velem(&v1, 3, 4), 360);
}

#[test]
fn test_vsub_vx_e32() {
    let asm = r#"
.text
main:
    vsub.vx v1, v2, t0
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 100);
    set_velem(&mut v2, 1, 4, 200);
    set_velem(&mut v2, 2, 4, 300);
    set_velem(&mut v2, 3, 4, 400);
    set_vreg(&mut m, "v2", v2);
    set_reg(&mut m, "t0", 5u64);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 95);
    assert_eq!(get_velem(&v1, 1, 4), 195);
    assert_eq!(get_velem(&v1, 2, 4), 295);
    assert_eq!(get_velem(&v1, 3, 4), 395);
}

// ============================================================
// vmul tests
// ============================================================

#[test]
fn test_vmul_vv_e32() {
    let asm = r#"
.text
main:
    vmul.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 3);
    set_velem(&mut v2, 1, 4, 5);
    set_velem(&mut v2, 2, 4, 7);
    set_velem(&mut v2, 3, 4, 11);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 10);
    set_velem(&mut v3, 1, 4, 20);
    set_velem(&mut v3, 2, 4, 30);
    set_velem(&mut v3, 3, 4, 40);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 30);
    assert_eq!(get_velem(&v1, 1, 4), 100);
    assert_eq!(get_velem(&v1, 2, 4), 210);
    assert_eq!(get_velem(&v1, 3, 4), 440);
}

#[test]
fn test_vmul_vx_e32() {
    let asm = r#"
.text
main:
    vmul.vx v1, v2, t0
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 3);
    set_velem(&mut v2, 1, 4, 5);
    set_velem(&mut v2, 2, 4, 7);
    set_velem(&mut v2, 3, 4, 11);
    set_vreg(&mut m, "v2", v2);
    set_reg(&mut m, "t0", 10u64);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 30);
    assert_eq!(get_velem(&v1, 1, 4), 50);
    assert_eq!(get_velem(&v1, 2, 4), 70);
    assert_eq!(get_velem(&v1, 3, 4), 110);
}

// ============================================================
// vdiv / vdivu tests
// ============================================================

#[test]
fn test_vdiv_vv_e32() {
    let asm = r#"
.text
main:
    vdiv.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 100);
    set_velem(&mut v2, 1, 4, (-50i32) as u32 as u64);
    set_velem(&mut v2, 2, 4, 7);
    set_velem(&mut v2, 3, 4, 0);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 7);
    set_velem(&mut v3, 1, 4, 3);
    set_velem(&mut v3, 2, 4, 0);
    set_velem(&mut v3, 3, 4, 5);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4) as i32 as i64, 14);       // 100 / 7 = 14
    assert_eq!(get_velem(&v1, 1, 4) as i32 as i64, -16);      // -50 / 3 = -16
    assert_eq!(get_velem(&v1, 2, 4), u32::MAX as u64);         // div by 0 = -1
    assert_eq!(get_velem(&v1, 3, 4), 0);                       // 0 / 5 = 0
}

#[test]
fn test_vdivu_vv_e32() {
    let asm = r#"
.text
main:
    vdivu.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 100u64);
    set_velem(&mut v2, 1, 4, 42u64);
    set_velem(&mut v2, 2, 4, 7u64);
    set_velem(&mut v2, 3, 4, 0u64);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 7u64);
    set_velem(&mut v3, 1, 4, 0u64);
    set_velem(&mut v3, 2, 4, 3u64);
    set_velem(&mut v3, 3, 4, 5u64);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 14);       // 100 / 7 = 14
    assert_eq!(get_velem(&v1, 1, 4), u32::MAX as u64); // div by 0 = -1
    assert_eq!(get_velem(&v1, 2, 4), 2);        // 7 / 3 = 2
    assert_eq!(get_velem(&v1, 3, 4), 0);        // 0 / 5 = 0
}

// ============================================================
// vrem / vremu tests
// ============================================================

#[test]
fn test_vrem_vv_e32() {
    let asm = r#"
.text
main:
    vrem.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 100);
    set_velem(&mut v2, 1, 4, (-100i32) as u32 as u64);
    set_velem(&mut v2, 2, 4, 7);
    set_velem(&mut v2, 3, 4, 42);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 7);
    set_velem(&mut v3, 1, 4, 7);
    set_velem(&mut v3, 2, 4, 0);
    set_velem(&mut v3, 3, 4, 5);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4) as i32 as i64, 2);       // 100 % 7 = 2
    assert_eq!(get_velem(&v1, 1, 4) as i32 as i64, -2);      // -100 % 7 = -2
    assert_eq!(get_velem(&v1, 2, 4), 7);                     // rem by 0 = dividend
    assert_eq!(get_velem(&v1, 3, 4), 2);                     // 42 % 5 = 2
}

#[test]
fn test_vremu_vv_e32() {
    let asm = r#"
.text
main:
    vremu.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 100u64);
    set_velem(&mut v2, 1, 4, 7u64);
    set_velem(&mut v2, 2, 4, 42u64);
    set_velem(&mut v2, 3, 4, 100u64);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 7u64);
    set_velem(&mut v3, 1, 4, 0u64);
    set_velem(&mut v3, 2, 4, 5u64);
    set_velem(&mut v3, 3, 4, 3u64);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 2);     // 100 % 7 = 2
    assert_eq!(get_velem(&v1, 1, 4), 7);     // rem by 0 = dividend
    assert_eq!(get_velem(&v1, 2, 4), 2);     // 42 % 5 = 2
    assert_eq!(get_velem(&v1, 3, 4), 1);     // 100 % 3 = 1
}

// ============================================================
// vmin / vmax tests
// ============================================================

#[test]
fn test_vmin_vv_e32() {
    let asm = r#"
.text
main:
    vmin.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 10);
    set_velem(&mut v2, 1, 4, (-5i32) as u32 as u64);
    set_velem(&mut v2, 2, 4, 100);
    set_velem(&mut v2, 3, 4, (-20i32) as u32 as u64);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 20);
    set_velem(&mut v3, 1, 4, 3);
    set_velem(&mut v3, 2, 4, 50);
    set_velem(&mut v3, 3, 4, 0);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 10);                       // min(10, 20) = 10
    assert_eq!(get_velem(&v1, 1, 4) as i32 as i64, -5);        // min(-5, 3) = -5
    assert_eq!(get_velem(&v1, 2, 4), 50);                       // min(100, 50) = 50
    assert_eq!(get_velem(&v1, 3, 4) as i32 as i64, -20);       // min(-20, 0) = -20
}

#[test]
fn test_vmax_vv_e32() {
    let asm = r#"
.text
main:
    vmax.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 10);
    set_velem(&mut v2, 1, 4, (-5i32) as u32 as u64);
    set_velem(&mut v2, 2, 4, 100);
    set_velem(&mut v2, 3, 4, (-20i32) as u32 as u64);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 20);
    set_velem(&mut v3, 1, 4, 3);
    set_velem(&mut v3, 2, 4, 50);
    set_velem(&mut v3, 3, 4, 0);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 20);                      // max(10, 20) = 20
    assert_eq!(get_velem(&v1, 1, 4) as i32 as i64, 3);         // max(-5, 3) = 3
    assert_eq!(get_velem(&v1, 2, 4), 100);                      // max(100, 50) = 100
    assert_eq!(get_velem(&v1, 3, 4) as i32 as i64, 0);         // max(-20, 0) = 0
}

#[test]
fn test_vminu_vv_e32() {
    let asm = r#"
.text
main:
    vminu.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 10u64);
    set_velem(&mut v2, 1, 4, 3u64);
    set_velem(&mut v2, 2, 4, 100u64);
    set_velem(&mut v2, 3, 4, 50u64);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 20u64);
    set_velem(&mut v3, 1, 4, 5u64);
    set_velem(&mut v3, 2, 4, 200u64);
    set_velem(&mut v3, 3, 4, 30u64);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 10);
    assert_eq!(get_velem(&v1, 1, 4), 3);
    assert_eq!(get_velem(&v1, 2, 4), 100);
    assert_eq!(get_velem(&v1, 3, 4), 30);
}

#[test]
fn test_vmaxu_vv_e32() {
    let asm = r#"
.text
main:
    vmaxu.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 10u64);
    set_velem(&mut v2, 1, 4, 3u64);
    set_velem(&mut v2, 2, 4, 100u64);
    set_velem(&mut v2, 3, 4, 50u64);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 20u64);
    set_velem(&mut v3, 1, 4, 5u64);
    set_velem(&mut v3, 2, 4, 200u64);
    set_velem(&mut v3, 3, 4, 30u64);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 20);
    assert_eq!(get_velem(&v1, 1, 4), 5);
    assert_eq!(get_velem(&v1, 2, 4), 200);
    assert_eq!(get_velem(&v1, 3, 4), 50);
}

// ============================================================
// vand / vor / vxor tests
// ============================================================

#[test]
fn test_vand_vv_e32() {
    let asm = r#"
.text
main:
    vand.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 0xFFu64);
    set_velem(&mut v2, 1, 4, 0x0Fu64);
    set_velem(&mut v2, 2, 4, 0xF0u64);
    set_velem(&mut v2, 3, 4, 0xAAu64);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 0xF0u64);
    set_velem(&mut v3, 1, 4, 0x00u64);
    set_velem(&mut v3, 2, 4, 0x0Fu64);
    set_velem(&mut v3, 3, 4, 0x55u64);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 0xF0);
    assert_eq!(get_velem(&v1, 1, 4), 0x00);
    assert_eq!(get_velem(&v1, 2, 4), 0x00);
    assert_eq!(get_velem(&v1, 3, 4), 0x00);
}

#[test]
fn test_vor_vv_e32() {
    let asm = r#"
.text
main:
    vor.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 0xF0u64);
    set_velem(&mut v2, 1, 4, 0x0Fu64);
    set_velem(&mut v2, 2, 4, 0x00u64);
    set_velem(&mut v2, 3, 4, 0xAAu64);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 0x0Fu64);
    set_velem(&mut v3, 1, 4, 0x00u64);
    set_velem(&mut v3, 2, 4, 0xFFu64);
    set_velem(&mut v3, 3, 4, 0x55u64);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 0xFF);
    assert_eq!(get_velem(&v1, 1, 4), 0x0F);
    assert_eq!(get_velem(&v1, 2, 4), 0xFF);
    assert_eq!(get_velem(&v1, 3, 4), 0xFF);
}

#[test]
fn test_vxor_vv_e32() {
    let asm = r#"
.text
main:
    vxor.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 0xFFu64);
    set_velem(&mut v2, 1, 4, 0xF0u64);
    set_velem(&mut v2, 2, 4, 0xAAu64);
    set_velem(&mut v2, 3, 4, 0x55u64);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 0xFFu64);
    set_velem(&mut v3, 1, 4, 0x0Fu64);
    set_velem(&mut v3, 2, 4, 0x55u64);
    set_velem(&mut v3, 3, 4, 0x55u64);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 0x00);
    assert_eq!(get_velem(&v1, 1, 4), 0xFF);
    assert_eq!(get_velem(&v1, 2, 4), 0xFF);
    assert_eq!(get_velem(&v1, 3, 4), 0x00);
}

// ============================================================
// vsll / vsrl / vsra tests
// ============================================================

#[test]
fn test_vsll_vv_e32() {
    let asm = r#"
.text
main:
    vsll.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 1);
    set_velem(&mut v2, 1, 4, 3);
    set_velem(&mut v2, 2, 4, 0x10);
    set_velem(&mut v2, 3, 4, 7);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 2);
    set_velem(&mut v3, 1, 4, 4);
    set_velem(&mut v3, 2, 4, 2);
    set_velem(&mut v3, 3, 4, 1);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 4);       // 1 << 2 = 4
    assert_eq!(get_velem(&v1, 1, 4), 48);      // 3 << 4 = 48
    assert_eq!(get_velem(&v1, 2, 4), 64);      // 0x10 << 2 = 64
    assert_eq!(get_velem(&v1, 3, 4), 14);      // 7 << 1 = 14
}

#[test]
fn test_vsrl_vv_e32() {
    let asm = r#"
.text
main:
    vsrl.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 0x80);
    set_velem(&mut v2, 1, 4, 0x10);
    set_velem(&mut v2, 2, 4, 64);
    set_velem(&mut v2, 3, 4, 15);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 4);
    set_velem(&mut v3, 1, 4, 2);
    set_velem(&mut v3, 2, 4, 1);
    set_velem(&mut v3, 3, 4, 2);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 8);       // 0x80 >> 4 = 8
    assert_eq!(get_velem(&v1, 1, 4), 4);       // 0x10 >> 2 = 4
    assert_eq!(get_velem(&v1, 2, 4), 32);      // 64 >> 1 = 32
    assert_eq!(get_velem(&v1, 3, 4), 3);       // 15 >> 2 = 3
}

#[test]
fn test_vsra_vv_e32() {
    let asm = r#"
.text
main:
    vsra.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    // -128 as signed 32-bit = 0xFFFFFF80
    set_velem(&mut v2, 0, 4, (-128i32) as u32 as u64);
    set_velem(&mut v2, 1, 4, (-16i32) as u32 as u64);
    set_velem(&mut v2, 2, 4, 64);
    set_velem(&mut v2, 3, 4, (-8i32) as u32 as u64);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 4);
    set_velem(&mut v3, 1, 4, 2);
    set_velem(&mut v3, 2, 4, 1);
    set_velem(&mut v3, 3, 4, 2);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    // -128 >> 4 = -8 as signed 32-bit = 0xFFFFFFF8
    assert_eq!(get_velem(&v1, 0, 4) as i32 as i64, -8);
    // -16 >> 2 = -4 as signed 32-bit
    assert_eq!(get_velem(&v1, 1, 4) as i32 as i64, -4);
    // 64 >> 1 = 32
    assert_eq!(get_velem(&v1, 2, 4), 32);
    // -8 >> 2 = -2 as signed 32-bit
    assert_eq!(get_velem(&v1, 3, 4) as i32 as i64, -2);
}

// ============================================================
// vmseq / vmslt / vmsle tests
// ============================================================

#[test]
fn test_vmseq_vv_e32() {
    let asm = r#"
.text
main:
    vmseq.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 10);
    set_velem(&mut v2, 1, 4, 20);
    set_velem(&mut v2, 2, 4, 30);
    set_velem(&mut v2, 3, 4, 40);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 10);
    set_velem(&mut v3, 1, 4, 21);
    set_velem(&mut v3, 2, 4, 30);
    set_velem(&mut v3, 3, 4, 39);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), u32::MAX as u64); // 10 == 10 -> true
    assert_eq!(get_velem(&v1, 1, 4), 0);                // 20 != 21 -> false
    assert_eq!(get_velem(&v1, 2, 4), u32::MAX as u64); // 30 == 30 -> true
    assert_eq!(get_velem(&v1, 3, 4), 0);                // 40 != 39 -> false
}

#[test]
fn test_vmslt_vv_e32() {
    let asm = r#"
.text
main:
    vmslt.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 5);
    set_velem(&mut v2, 1, 4, (-5i32) as u32 as u64);
    set_velem(&mut v2, 2, 4, 10);
    set_velem(&mut v2, 3, 4, 0);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 10);
    set_velem(&mut v3, 1, 4, 0);
    set_velem(&mut v3, 2, 4, 10);
    set_velem(&mut v3, 3, 4, (-1i32) as u32 as u64);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), u32::MAX as u64); // 5 < 10 -> true
    assert_eq!(get_velem(&v1, 1, 4), u32::MAX as u64); // -5 < 0 -> true
    assert_eq!(get_velem(&v1, 2, 4), 0);                // 10 < 10 -> false
    assert_eq!(get_velem(&v1, 3, 4), 0);                // 0 < -1 -> false
}

#[test]
fn test_vmsle_vv_e32() {
    let asm = r#"
.text
main:
    vmsle.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 5);
    set_velem(&mut v2, 1, 4, 10);
    set_velem(&mut v2, 2, 4, 10);
    set_velem(&mut v2, 3, 4, 15);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 10);
    set_velem(&mut v3, 1, 4, 10);
    set_velem(&mut v3, 2, 4, 5);
    set_velem(&mut v3, 3, 4, 10);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), u32::MAX as u64); // 5 <= 10 -> true
    assert_eq!(get_velem(&v1, 1, 4), u32::MAX as u64); // 10 <= 10 -> true
    assert_eq!(get_velem(&v1, 2, 4), 0);                // 10 <= 5 -> false
    assert_eq!(get_velem(&v1, 3, 4), 0);                // 15 <= 10 -> false
}

// ============================================================
// vmerge / vmv tests
// ============================================================

#[test]
fn test_vmerge_vv_e32() {
    let asm = r#"
.text
main:
    vmerge.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 10);
    set_velem(&mut v2, 1, 4, 20);
    set_velem(&mut v2, 2, 4, 30);
    set_velem(&mut v2, 3, 4, 40);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 100);
    set_velem(&mut v3, 1, 4, 200);
    set_velem(&mut v3, 2, 4, 300);
    set_velem(&mut v3, 3, 4, 400);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    // merge copies from vs2, then vs1 elements replace where mask=1
    // Without mask, result is vs2 (base) with vs1 elements
    assert_eq!(get_velem(&v1, 0, 4), 100);
    assert_eq!(get_velem(&v1, 1, 4), 200);
    assert_eq!(get_velem(&v1, 2, 4), 300);
    assert_eq!(get_velem(&v1, 3, 4), 400);
}

#[test]
fn test_vmv_v_v_e32() {
    // vmv.v.v v1, v3, v2  (VV format: rd, rs1, rs2)
    // vmv ignores vs2 (rs1) and copies vs1 (rs2) to vd (rd)
    let asm = r#"
.text
main:
    vmv.v.v v1, v3, v2
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    // v2 is the source to copy (rs2 = vs1)
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 42);
    set_velem(&mut v2, 1, 4, 99);
    set_velem(&mut v2, 2, 4, 123);
    set_velem(&mut v2, 3, 4, 255);
    set_vreg(&mut m, "v2", v2);
    // v3 is dummy, ignored by vmv
    let v3 = vec![0u8; 64];
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 42);
    assert_eq!(get_velem(&v1, 1, 4), 99);
    assert_eq!(get_velem(&v1, 2, 4), 123);
    assert_eq!(get_velem(&v1, 3, 4), 255);
}

// ============================================================
// vload tests
// ============================================================

#[test]
fn test_vle32_v() {
    let asm = r#"
.text
main:
    vle32.v v1, (t0)
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    // Write data to memory at address 0x1000
    let base = 0x1000u64;
    set_reg(&mut m, "t0", base);
    // Write 4 u32 values to memory
    m.memory.write_u32(base, 100);
    m.memory.write_u32(base + 4, 200);
    m.memory.write_u32(base + 8, 300);
    m.memory.write_u32(base + 12, 400);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), 100);
    assert_eq!(get_velem(&v1, 1, 4), 200);
    assert_eq!(get_velem(&v1, 2, 4), 300);
    assert_eq!(get_velem(&v1, 3, 4), 400);
}

#[test]
fn test_vle64_v() {
    let asm = r#"
.text
main:
    vle64.v v1, (t0)
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e64", "m1", 2);
    let base = 0x1000u64;
    set_reg(&mut m, "t0", base);
    m.memory.write_u64(base, 0xDEADBEEF);
    m.memory.write_u64(base + 8, 0xCAFEBABE);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 8), 0xDEADBEEF);
    assert_eq!(get_velem(&v1, 1, 8), 0xCAFEBABE);
}

#[test]
fn test_vle8_v() {
    let asm = r#"
.text
main:
    vle8.v v1, (t0)
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e8", "m1", 8);
    let base = 0x1000u64;
    set_reg(&mut m, "t0", base);
    for i in 0..8 {
        m.memory.write_u8(base + i as u64, (i * 10 + 1) as u8);
    }
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 1), 1);
    assert_eq!(get_velem(&v1, 1, 1), 11);
    assert_eq!(get_velem(&v1, 7, 1), 71);
}

// ============================================================
// vstore tests
// ============================================================

#[test]
fn test_vse32_v() {
    let asm = r#"
.text
main:
    vse32.v v1, (t0)
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let base = 0x1000u64;
    set_reg(&mut m, "t0", base);
    // Setup v1 with data
    let mut v1 = vec![0u8; 64];
    set_velem(&mut v1, 0, 4, 111);
    set_velem(&mut v1, 1, 4, 222);
    set_velem(&mut v1, 2, 4, 333);
    set_velem(&mut v1, 3, 4, 444);
    set_vreg(&mut m, "v1", v1);
    step(&mut m);
    // Verify memory was written
    assert_eq!(m.memory.read_u32(base), 111);
    assert_eq!(m.memory.read_u32(base + 4), 222);
    assert_eq!(m.memory.read_u32(base + 8), 333);
    assert_eq!(m.memory.read_u32(base + 12), 444);
}

#[test]
fn test_vse64_v() {
    let asm = r#"
.text
main:
    vse64.v v1, (t0)
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e64", "m1", 2);
    let base = 0x1000u64;
    set_reg(&mut m, "t0", base);
    let mut v1 = vec![0u8; 64];
    set_velem(&mut v1, 0, 8, 0xAAAABBBBCCCCDDDD);
    set_velem(&mut v1, 1, 8, 0x1111222233334444);
    set_vreg(&mut m, "v1", v1);
    step(&mut m);
    assert_eq!(m.memory.read_u64(base), 0xAAAABBBBCCCCDDDD);
    assert_eq!(m.memory.read_u64(base + 8), 0x1111222233334444);
}

// ============================================================
// vredsum tests
// ============================================================

#[test]
fn test_vredsum_vs_e32() {
    let asm = r#"
.text
main:
    vredsum.vs v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 1);
    set_velem(&mut v2, 1, 4, 2);
    set_velem(&mut v2, 2, 4, 3);
    set_velem(&mut v2, 3, 4, 4);
    set_vreg(&mut m, "v2", v2);
    // v3[0] = initial value (scalar)
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 10);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    // 10 + 1 + 2 + 3 + 4 = 20
    assert_eq!(get_velem(&v1, 0, 4), 20);
}

// ============================================================
// vmask tests (vand.mm, vor.mm, vxor.mm, vnot.m)
// ============================================================

#[test]
fn test_vand_mm() {
    let asm = r#"
.text
main:
    vand.mm v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e8", "m1", 8);
    let mut v2 = vec![0u8; 64];
    for i in 0..4 {
        v2[i] = 0xFF;
    }
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    v3[0] = 0xFF;
    v3[2] = 0xFF;
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(v1[0], 0xFF);
    assert_eq!(v1[1], 0x00);
    assert_eq!(v1[2], 0xFF);
    assert_eq!(v1[3], 0x00);
}

#[test]
fn test_vor_mm() {
    let asm = r#"
.text
main:
    vor.mm v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e8", "m1", 8);
    let mut v2 = vec![0u8; 64];
    v2[0] = 0xFF;
    v2[2] = 0xFF;
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    v3[1] = 0xFF;
    v3[2] = 0xFF;
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(v1[0], 0xFF);
    assert_eq!(v1[1], 0xFF);
    assert_eq!(v1[2], 0xFF);
    assert_eq!(v1[3], 0x00);
}

#[test]
fn test_vxor_mm() {
    let asm = r#"
.text
main:
    vxor.mm v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e8", "m1", 8);
    let mut v2 = vec![0u8; 64];
    v2[0] = 0xFF;
    v2[1] = 0xFF;
    v2[2] = 0xFF;
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    v3[0] = 0xFF;
    v3[2] = 0xFF;
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(v1[0], 0x00); // 0xFF ^ 0xFF = 0
    assert_eq!(v1[1], 0xFF); // 0xFF ^ 0 = 0xFF
    assert_eq!(v1[2], 0x00); // 0xFF ^ 0xFF = 0
}

#[test]
fn test_vnot_m() {
    let asm = r#"
.text
main:
    vnot.m v1, v2
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e8", "m1", 8);
    let mut v2 = vec![0u8; 64];
    v2[0] = 0xFF;
    v2[1] = 0x00;
    v2[2] = 0xFF;
    set_vreg(&mut m, "v2", v2);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(v1[0], 0x00);
    assert_eq!(v1[1], 0xFF);
    assert_eq!(v1[2], 0x00);
}

// ============================================================
// vmsltu / vmsgtu - unsigned mask compare
// ============================================================

#[test]
fn test_vmsltu_vv_e32() {
    let asm = r#"
.text
main:
    vmsltu.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 10);
    set_velem(&mut v2, 1, 4, 100);
    set_velem(&mut v2, 2, 4, 50);
    set_velem(&mut v2, 3, 4, 0);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 20);
    set_velem(&mut v3, 1, 4, 50);
    set_velem(&mut v3, 2, 4, 100);
    set_velem(&mut v3, 3, 4, 0);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), u32::MAX as u64);  // 10 < 20
    assert_eq!(get_velem(&v1, 1, 4), 0);                // 100 >= 50
    assert_eq!(get_velem(&v1, 2, 4), u32::MAX as u64);  // 50 < 100
    assert_eq!(get_velem(&v1, 3, 4), 0);                // 0 == 0, not less than
}

#[test]
fn test_vmsgtu_vv_e32() {
    let asm = r#"
.text
main:
    vmsgtu.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 100);
    set_velem(&mut v2, 1, 4, 10);
    set_velem(&mut v2, 2, 4, 50);
    set_velem(&mut v2, 3, 4, 0);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 50);
    set_velem(&mut v3, 1, 4, 100);
    set_velem(&mut v3, 2, 4, 50);
    set_velem(&mut v3, 3, 4, 1);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 4), u32::MAX as u64);  // 100 > 50
    assert_eq!(get_velem(&v1, 1, 4), 0);                // 10 <= 100
    assert_eq!(get_velem(&v1, 2, 4), 0);                // 50 == 50
    assert_eq!(get_velem(&v1, 3, 4), 0);                // 0 < 1
}

// ============================================================
// vredmin / vredmax - reduction min/max
// ============================================================

#[test]
fn test_vredmin_vs_e32() {
    let asm = r#"
.text
main:
    vredmin.vs v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 42);
    set_velem(&mut v2, 1, 4, (-10i32) as u32 as u64);
    set_velem(&mut v2, 2, 4, 100);
    set_velem(&mut v2, 3, 4, (-5i32) as u32 as u64);
    set_vreg(&mut m, "v2", v2);
    // vs1 provides initial value
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 0);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    // signed min: min(0, 42, -10, 100, -5) = -10
    assert_eq!(get_velem(&v1, 0, 4), (-10i32) as u32 as u64);
}

#[test]
fn test_vredmax_vs_e32() {
    let asm = r#"
.text
main:
    vredmax.vs v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 42);
    set_velem(&mut v2, 1, 4, (-10i32) as u32 as u64);
    set_velem(&mut v2, 2, 4, 100);
    set_velem(&mut v2, 3, 4, (-5i32) as u32 as u64);
    set_vreg(&mut m, "v2", v2);
    // vs1 provides initial value
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 0);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    // signed max: max(0, 42, -10, 100, -5) = 100
    assert_eq!(get_velem(&v1, 0, 4), 100);
}

// ============================================================
// Edge cases
// ============================================================

#[test]
fn test_vector_vl_zero_noop() {
    let asm = r#"
.text
main:
    vadd.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e32", "m1", 0);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 4, 1);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 4, 2);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    // VL=0, no elements processed, v1 should be all zeros
    assert_eq!(get_velem(&v1, 0, 4), 0);
}

#[test]
fn test_vadd_vv_e64() {
    let asm = r#"
.text
main:
    vadd.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e64", "m1", 2);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 8, 0x100000000u64);
    set_velem(&mut v2, 1, 8, 0x200000000u64);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 8, 1);
    set_velem(&mut v3, 1, 8, 2);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 8), 0x100000001);
    assert_eq!(get_velem(&v1, 1, 8), 0x200000002);
}

#[test]
fn test_vadd_vv_e16() {
    let asm = r#"
.text
main:
    vadd.vv v1, v2, v3
"#;
    let mut m = setup_machine(asm);
    setup_vsetvli(&mut m, "e16", "m1", 4);
    let mut v2 = vec![0u8; 64];
    set_velem(&mut v2, 0, 2, 100);
    set_velem(&mut v2, 1, 2, 200);
    set_velem(&mut v2, 2, 2, 300);
    set_velem(&mut v2, 3, 2, 400);
    set_vreg(&mut m, "v2", v2);
    let mut v3 = vec![0u8; 64];
    set_velem(&mut v3, 0, 2, 10);
    set_velem(&mut v3, 1, 2, 20);
    set_velem(&mut v3, 2, 2, 30);
    set_velem(&mut v3, 3, 2, 40);
    set_vreg(&mut m, "v3", v3);
    step(&mut m);
    let v1 = get_vreg(&m, "v1");
    assert_eq!(get_velem(&v1, 0, 2), 110);
    assert_eq!(get_velem(&v1, 1, 2), 220);
    assert_eq!(get_velem(&v1, 2, 2), 330);
    assert_eq!(get_velem(&v1, 3, 2), 440);
}
