
use riscv_asm_lib::r5asm::asm_program::AsmProgram;
use riscv_asm_lib::r5asm::imm::Imm::{self};
use riscv_asm_lib::r5asm::instruction::Instruction;
use riscv_asm_lib::r5asm::opcode::OpCode;
use riscv_asm_lib::r5asm::register::Register;

use crate::debugger_error::DebuggerError;
use crate::machine::hart::{Hart, HartPerformance, HartState, PC_INCREMENT, PrivilegeLevel, EXCEPTION_ENVIRONMENT_CALL_FROM_U, EXCEPTION_ENVIRONMENT_CALL_FROM_S, EXCEPTION_ENVIRONMENT_CALL_FROM_M, EXCEPTION_BREAKPOINT};
use crate::machine::processor::Processor;
use crate::machine::register_ref::{RegisterRef, RegisterType};
use crate::memory::memory::Memory;

use std::collections::HashMap;

// ============================================================
// AES S-Box and helper tables for RISC-V krypto instructions
// ============================================================

/// AES forward S-Box (SubBytes step)
const AES_SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

/// AES inverse S-Box (InvSubBytes step)
const AES_INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
    0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
    0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
    0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
    0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
    0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
    0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
    0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
    0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
    0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
    0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
    0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
    0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
    0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
    0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
];

/// AES key schedule round constants (RCON)
const AES_RCON: [u8; 11] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36, 0x00];

/// Apply the AES forward S-Box to each byte of a 64-bit value (SubBytes step)
fn aes64_sub_bytes(value: u64) -> u64 {
    let mut result: u64 = 0;
    for i in 0..8 {
        let byte = ((value >> (i * 8)) & 0xFF) as u8;
        let subbed = AES_SBOX[byte as usize] as u64;
        result |= subbed << (i * 8);
    }
    result
}

/// Apply the AES inverse S-Box to each byte of a 64-bit value (InvSubBytes step)
fn aes64_inv_sub_bytes(value: u64) -> u64 {
    let mut result: u64 = 0;
    for i in 0..8 {
        let byte = ((value >> (i * 8)) & 0xFF) as u8;
        let subbed = AES_INV_SBOX[byte as usize] as u64;
        result |= subbed << (i * 8);
    }
    result
}

/// Galois Field multiplication by {02} in GF(2^8) with polynomial x^8 + x^4 + x^3 + x + 1
fn gf_mul2(b: u8) -> u8 {
    let r = (b as u16) << 1;
    if (b & 0x80) != 0 {
        (r ^ 0x11b) as u8
    } else {
        r as u8
    }
}

/// Galois Field multiplication in GF(2^8) with polynomial x^8 + x^4 + x^3 + x + 1
fn gf_mul(a: u8, b: u8) -> u8 {
    let mut p: u8 = 0;
    let mut a_mut = a;
    let mut b_mut = b;
    for _ in 0..8 {
        if (b_mut & 1) != 0 {
            p ^= a_mut;
        }
        let hi_bit_set = (a_mut & 0x80) != 0;
        a_mut <<= 1;
        if hi_bit_set {
            a_mut ^= 0x1b;
        }
        b_mut >>= 1;
    }
    p
}

/// Apply MixColumns to a single 32-bit column (4 bytes as u32, little-endian)
/// AES MixColumns: each output byte = {02}*b0 + {03}*b1 + {01}*b2 + {01}*b3 (rotated)
fn mix_column(col: u32) -> u32 {
    let b0 = (col & 0xFF) as u8;
    let b1 = ((col >> 8) & 0xFF) as u8;
    let b2 = ((col >> 16) & 0xFF) as u8;
    let b3 = ((col >> 24) & 0xFF) as u8;

    let r0 = gf_mul2(b0) ^ gf_mul(0x03, b1) ^ b2 ^ b3;
    let r1 = b0 ^ gf_mul2(b1) ^ gf_mul(0x03, b2) ^ b3;
    let r2 = b0 ^ b1 ^ gf_mul2(b2) ^ gf_mul(0x03, b3);
    let r3 = gf_mul(0x03, b0) ^ b1 ^ b2 ^ gf_mul2(b3);

    (r0 as u32) | ((r1 as u32) << 8) | ((r2 as u32) << 16) | ((r3 as u32) << 24)
}

/// Apply InvMixColumns to a single 32-bit column (4 bytes as u32, little-endian)
/// AES InvMixColumns: each output byte = {0e}*b0 + {0b}*b1 + {0d}*b2 + {09}*b3 (rotated)
fn inv_mix_column(col: u32) -> u32 {
    let b0 = (col & 0xFF) as u8;
    let b1 = ((col >> 8) & 0xFF) as u8;
    let b2 = ((col >> 16) & 0xFF) as u8;
    let b3 = ((col >> 24) & 0xFF) as u8;

    let r0 = gf_mul(0x0e, b0) ^ gf_mul(0x0b, b1) ^ gf_mul(0x0d, b2) ^ gf_mul(0x09, b3);
    let r1 = gf_mul(0x09, b0) ^ gf_mul(0x0e, b1) ^ gf_mul(0x0b, b2) ^ gf_mul(0x0d, b3);
    let r2 = gf_mul(0x0d, b0) ^ gf_mul(0x09, b1) ^ gf_mul(0x0e, b2) ^ gf_mul(0x0b, b3);
    let r3 = gf_mul(0x0b, b0) ^ gf_mul(0x0d, b1) ^ gf_mul(0x09, b2) ^ gf_mul(0x0e, b3);

    (r0 as u32) | ((r1 as u32) << 8) | ((r2 as u32) << 16) | ((r3 as u32) << 24)
}

/// AES MixColumns on 64-bit value: treats as two independent 32-bit columns
fn aes64_mix_columns(value: u64) -> u64 {
    let lo = mix_column((value & 0xFFFF_FFFF) as u32);
    let hi = mix_column(((value >> 32) & 0xFFFF_FFFF) as u32);
    (lo as u64) | ((hi as u64) << 32)
}

/// AES InvMixColumns on 64-bit value: treats as two independent 32-bit columns
fn aes64_inv_mix_columns(value: u64) -> u64 {
    let lo = inv_mix_column((value & 0xFFFF_FFFF) as u32);
    let hi = inv_mix_column(((value >> 32) & 0xFFFF_FFFF) as u32);
    (lo as u64) | ((hi as u64) << 32)
}

/// AES Key Schedule step 1 (round constant): takes rs1 as {w1, w0} (two 32-bit words),
/// rotates w1 left 32 bits, applies SubBytes, XORs rcon into top byte,
/// returns the new w1 in low 32 bits, sign-extended to 64 bits.
fn aes64_ks1i(rs1: u64, rcon: u32) -> u64 {
    // Extract w1 (high 32 bits of rs1)
    let w1: u32 = (rs1 >> 32) as u32;
    // Rotate w1 right by 8 bits (equivalent to left rotate by 24 in 32-bit)
    // Actually per spec: w1 = RotateWord(w1) = w1.rotate_left(8) or w1.rotate_right(24)
    // But actually, aes64ks1i does: w1 = SubWord(RotWord(w1)) ^ rcon
    // RotWord: byte rotation left by 8 bits within 32-bit word
    let rotated = w1.rotate_left(8);
    // SubBytes on each byte of rotated w1
    let mut subbed: u32 = 0;
    for i in 0..4 {
        let byte = ((rotated >> (i * 8)) & 0xFF) as u8;
        subbed |= (AES_SBOX[byte as usize] as u32) << (i * 8);
    }
    // Look up actual RCON value from table, XOR into top byte (byte 3)
    let rcon_val = AES_RCON.get(rcon as usize).copied().unwrap_or(0);
    subbed ^= (rcon_val as u32) << 24;
    // Return result sign-extended from 32 bits
    subbed as i32 as i64 as u64
}

/// AES Key Schedule step 2: XOR two 64-bit values
fn aes64_ks2(rs1: u64, rs2: u64) -> u64 {
    rs1 ^ rs2
}

pub type ProcessorId = usize;
pub type HartId = u64;
pub type ProgramId = usize;

pub struct Machine {
    pub processors: Vec<Processor>,

    pub programs: Vec<AsmProgram>,

    pub registers: Register,

    /// O(1) cache: (program_id, pc_offset) → (index in text_section_items, machine_code_count)
    /// machine_code_count is the number of machine-code words the assembler produced for
    /// this item (1 for normal instructions, ≥2 for expanded pseudo-instructions like `la`).
    /// All offsets covered by the machine-code expansion are mapped to the same index,
    /// so the post-execution PC-validity check doesn't falsely detect "program finished"
    /// when PC lands on an intermediate offset within a pseudo-instruction expansion.
    inst_cache: HashMap<(ProgramId, usize), (usize, usize)>,

    /// Monotonically increasing counter for allocating globally unique Hart IDs.
    next_hart_id: u64,
}

impl Machine {

    pub fn new() -> Self {
        let mut r = Self {
            processors: vec![],
            programs: vec![],
            registers: Register::new(),
            inst_cache: HashMap::new(),
            next_hart_id: 0,       // hart 0 is created by default processor
        };

        r.add_processor(Processor::default());
        r
    }

    /// Register a program (parse, cache instructions) without loading it into any processor memory.
    /// Use `load_program_to_processor` to load the program into one or more processors.
    pub fn add_program(&mut self, program: AsmProgram) -> Result<ProgramId, DebuggerError> {
        let id = self.programs.len();

        // Build O(1) instruction cache for fast PC→instruction lookup.
        // For each text_section_item, resolve the full machine-code list
        // (pseudo-instructions like `la` may expand to multiple machine-code
        // words) and insert EVERY offset into the cache.  Without this,
        // intermediate offsets within a pseudo-instruction expansion would
        // cause fetch_inst to return None, and the hart would be marked
        // Finished prematurely.
        let labels = program.get_labels();
        for (idx, item) in program.get_text_section_items().iter().enumerate() {
            if item.get_inc().is_none() {
                continue;
            }
            let machine_codes = program
                .get_machine_code_list(item, &self.registers, &labels)
                .unwrap_or_default();
            let count = machine_codes.len().max(1); // at least 1 to prevent underflow
            let mut offset = item.get_offset();
            for mc in &machine_codes {
                let bytes = mc.get_code_data().to_vec();
                if !bytes.is_empty() {
                    self.inst_cache.insert((id, offset), (idx, count));
                    // P1-18: Use PC_INCREMENT (4) instead of bytes.len() to stay
                    // consistent with next_pc() which always advances by 4.
                    // If RISC-V compressed instructions (2 bytes) are added in
                    // the future, both cache and PC advancement must be updated
                    // together — using different values in each place creates a
                    // silent mismatch that causes every instruction lookup to fail.
                    offset += PC_INCREMENT;
                }
            }
            // If no machine codes were produced (shouldn't happen for a valid
            // instruction), still insert the start offset so the cache is
            // populated.
            let all_empty = machine_codes.iter().all(|mc| mc.get_code_data().to_vec().is_empty());
            if machine_codes.is_empty() || all_empty {
                self.inst_cache.insert((id, item.get_offset()), (idx, 1));
            }
        }

        self.programs.push(program);

        Ok(id)
    }

    /// Load a previously registered program into a specific processor's local memory.
    /// The same program can be loaded into multiple processors.
    /// If `update_harts` is true, all harts in this processor will have their
    /// program_id set to `program_id`. This should be true when loading a program
    /// into a processor for the first time, and false when the caller manages
    /// program_id assignment manually.
    pub fn load_program_to_processor(
        &mut self,
        program_id: ProgramId,
        proc_idx: usize,
        update_harts: bool,
    ) -> Result<(), DebuggerError> {
        if program_id >= self.programs.len() {
            return Err(DebuggerError::GeneralError(format!(
                "program_id {} out of range ({} programs)",
                program_id,
                self.programs.len()
            )));
        }
        if proc_idx >= self.processors.len() {
            return Err(DebuggerError::GeneralError(format!(
                "proc_idx {} out of range ({} processors)",
                proc_idx,
                self.processors.len()
            )));
        }

        // Split borrows: immutable on self.programs + self.registers,
        // mutable on self.processors[proc_idx].memory.
        // Rust's field-level borrowing allows this because they are disjoint fields.
        let program = &self.programs[program_id];
        let registers = &self.registers;
        let memory = &mut self.processors[proc_idx].memory;

        let result = Self::load_program_into_memory(memory, program, registers);

        if update_harts {
            for hart in &mut self.processors[proc_idx].harts {
                hart.program_id = program_id;
            }
        }

        result
    }

    /// Static helper: writes program sections into the given memory.
    /// Takes explicit parameters to avoid borrowing self.
    fn load_program_into_memory(
        memory: &mut Memory,
        program: &AsmProgram,
        registers: &Register,
    ) -> Result<(), DebuggerError> {
        // Load non-text sections (data, bss, etc.)
        for item in program.get_non_text_section_items() {
            let Some(directive) = item.get_directive() else {
                continue;
            };

            let Some(machine_code) = directive.get_machine_code() else {
                continue;
            };

            let bytes = machine_code.get_code_data().to_vec();
            if bytes.is_empty() {
                continue;
            }

            memory.write_bytes(item.get_offset() as u64, &bytes)?;
        }

        // Load text section instruction machine code into memory.
        // Without this, code-region memory reads (PC-relative constant pools,
        // self-modifying code, debugger code segment inspection) return garbage.
        let labels = program.get_labels();
        for item in program.get_text_section_items() {
            let Some(_) = item.get_inc() else {
                continue;
            };
            let machine_codes = match program.get_machine_code_list(item, registers, &labels) {
                Ok(mc) => mc,
                Err(_) => continue,
            };
            let mut offset = item.get_offset() as u64;
            for mc in &machine_codes {
                let bytes = mc.get_code_data().to_vec();
                if !bytes.is_empty() {
                    memory.write_bytes(offset, &bytes)?;
                    offset += PC_INCREMENT as u64;
                }
            }
        }

        Ok(())
    }

    /// Add a processor, reassigning globally unique Hart IDs to every Hart
    /// inside it to prevent ID collisions across processors, and assigning a
    /// globally unique Processor ID.
    /// The new processor's clock is synchronised to the current global level
    /// so the "all processors share the same clock" invariant is maintained.
    pub fn add_processor(&mut self, mut processor: Processor) {
        // Reassign globally unique Hart IDs inside this processor
        for hart in &mut processor.harts {
            hart.id = self.next_hart_id;
            hart.csr.mhartid = self.next_hart_id;
            self.next_hart_id += 1;
        }

        // Synchronise the new processor's clock to the current global level.
        // All processors must share the same clock per architectural invariant.
        if let Some(first) = self.processors.first() {
            processor.set_clock(first.clock());
        }

        // Always sync hart start_clocks so runtime_cycles starts from 0,
        // matching elapsed_cycles semantics for newly created harts.
        // This applies to both the first processor (where clock == 0 ==
        // default start_clock) and subsequent processors (where clock has
        // advanced).
        let proc_clock = processor.clock();
        for hart in &mut processor.harts {
            hart.start_clock = proc_clock;
            // P0-1: Align elapsed_cycles to the processor clock so the new hart
            // shares the same time baseline as all existing harts. Without this,
            // elapsed_cycles starts from 0 and is permanently behind by
            // proc_clock cycles, violating the global-clock invariant.
            hart.elapsed_cycles = proc_clock;
        }

        self.processors.push(processor);
    }

    /// Find the index of the processor that owns the given hart.
    /// Returns None if no processor contains this hart.
    pub fn get_processor_index(&self, hart_id: HartId) -> Option<usize> {
        self.processors
            .iter()
            .position(|p| p.harts.iter().any(|h| h.id == hart_id))
    }

    pub fn get_hart(&self, hart_id: HartId) -> Option<&Hart> {
        for processor in &self.processors {
            for hart in &processor.harts {
                if hart.id == hart_id {
                    return Some(hart);
                }
            }
        }

        None
    }

    pub fn get_hart_mut(&mut self, hart_id: HartId) -> Option<&mut Hart> {
        for processor in &mut self.processors {

            for hart in &mut processor.harts {

                if hart.id == hart_id {

                    return Some(hart);
                }
            }
        }

        None
    }

    pub fn get_processor_from_hart_id(&self, hart_id: HartId) -> Option<&Processor> {
        for processor in &self.processors {
            for hart in &processor.harts {
                if hart.id == hart_id {
                    return Some(processor);
                }
            }
        }

        None
    }

    pub fn get_default_hart_id(&self) -> HartId {
        0
    }

    pub fn get_default_hart(&self) -> Option<&Hart> {
        self.get_hart(self.get_default_hart_id())
    }

    pub fn get_default_hart_mut(&mut self) -> Option<&mut Hart> {
        let default_hart_id = self.get_default_hart_id();
        self.get_hart_mut(default_hart_id)
    }

    pub fn get_hart_cycle_count(&self, hart_id: HartId) -> Option<u64> {
        self.get_hart(hart_id).map(|h| h.exec_cycles)
    }

    pub fn get_hart_inst_count(&self, hart_id: HartId) -> Option<u64> {
        self.get_hart(hart_id).map(|h| h.inst_count)
    }

    /// Return a unified performance snapshot for a single hart.
    /// All business logic (IPC calculation, state interpretation) lives
    /// inside the simulator so the DAP layer never computes metrics.
    pub fn get_hart_performance(&self, hart_id: HartId) -> Option<HartPerformance> {
        let hart = self.get_hart(hart_id)?;
        let processor_id = self.get_processor_index(hart_id).unwrap_or(0);
        let proc_clock = self.processors[processor_id].clock();
        // Freeze runtime_cycles at finish time for Finished harts,
        // preventing it from growing indefinitely after termination.
        let clock_limit = hart.finish_clock.map_or(proc_clock, |fc| fc.min(proc_clock));
        let runtime_cycles = clock_limit.saturating_sub(hart.start_clock);
        Some(HartPerformance {
            hart_id: hart.id,
            processor_id,
            state: hart.state.clone(),
            elapsed_cycles: hart.elapsed_cycles,
            exec_cycles: hart.exec_cycles,
            inst_count: hart.inst_count,
            runtime_cycles,
        })
    }

    /// Return performance snapshots for all harts.
    pub fn get_all_performances(&self) -> Vec<HartPerformance> {
        let mut result = Vec::new();
        for (pid, processor) in self.processors.iter().enumerate() {
            for hart in &processor.harts {
                let proc_clock = processor.clock();
                let clock_limit = hart.finish_clock.map_or(proc_clock, |fc| fc.min(proc_clock));
                let runtime_cycles = clock_limit.saturating_sub(hart.start_clock);
                result.push(HartPerformance {
                    hart_id: hart.id,
                    processor_id: pid,
                    state: hart.state.clone(),
                    elapsed_cycles: hart.elapsed_cycles,
                    exec_cycles: hart.exec_cycles,
                    inst_count: hart.inst_count,
                    runtime_cycles,
                });
            }
        }
        result
    }

    /// Return the clock value of processor 0 (backward-compatible convenience).
    ///
    /// NOTE: This is NOT a "global" clock. In multi-processor systems each
    /// processor has an independent clock. The name is retained for backward
    /// compatibility. Use `get_processor_clock` to get a specific processor's
    /// clock, and `get_all_performances` for per-hart/per-processor statistics.
    pub fn get_global_clock(&self) -> u64 {
        self.processors.first().map(|p| p.clock()).unwrap_or(0)
    }

    /// Return the clock value for a specific processor.
    pub fn get_processor_clock(&self, proc_idx: usize) -> Option<u64> {
        self.processors.get(proc_idx).map(|p| p.clock())
    }

    /// Return total number of processors.
    pub fn processor_count(&self) -> usize {
        self.processors.len()
    }

    /// Return total execution cycles across all harts (sum of per-hart exec_cycles).
    pub fn get_total_cycles(&self) -> u64 {
        self.processors
            .iter()
            .flat_map(|p| &p.harts)
            .map(|h| h.exec_cycles)
            .sum()
    }

    /// Return total number of harts across all processors.
    pub fn hart_count(&self) -> usize {
        self.processors.iter().map(|p| p.harts.len()).sum()
    }

    /// Return average cycles per hart. Returns 0 when there are no harts.
    pub fn get_avg_cycles_per_hart(&self) -> u64 {
        let count = self.hart_count() as u64;
        if count > 0 {
            self.get_total_cycles() / count
        } else {
            0
        }
    }

    /// Return all harts as a flat list of (processor_id, hart) references.
    pub fn get_all_harts(&self) -> Vec<(ProcessorId, &Hart)> {
        let mut result = Vec::new();
        for (pid, processor) in self.processors.iter().enumerate() {
            for hart in &processor.harts {
                result.push((pid, hart));
            }
        }
        result
    }

    /// Step all harts one instruction each, advancing every processor's clock
    /// by the **same** global maximum cycle count.  The invariant "all
    /// processors share the same clock" is strictly maintained.
    /// Returns the total number of harts that successfully stepped.
    pub fn step_all_harts(&mut self) -> Result<usize, DebuggerError> {
        let num_processors = self.processors.len();

        // Pre-collect hart lists to avoid borrow conflicts later
        let mut proc_harts: Vec<Vec<(HartId, bool)>> = (0..num_processors)
            .map(|pi| {
                self.processors[pi]
                    .harts
                    .iter()
                    .map(|h| (h.id, matches!(h.state, HartState::Finished)))
                    .collect()
            })
            .collect();

        let mut total_stepped = 0;
        let mut errors: Vec<String> = Vec::new();
        let mut per_proc_ok_cycles: Vec<Vec<(HartId, u64)>> = vec![Vec::new(); num_processors];
        /// Harts that transitioned to Finished during Pass 1 (per processor).
        /// Used in Pass 2 to correct finish_clock after the global clock advance.
        let mut per_proc_finished_this_round: Vec<Vec<HartId>> = vec![Vec::new(); num_processors];
        let mut global_max_cycles: u64 = 0;
        let mut any_stepped = false;

        // ── Pass 1: execute all harts, collect per-hart cycle costs ──
        for proc_idx in 0..num_processors {
            let hart_info = std::mem::take(&mut proc_harts[proc_idx]);
            let mut proc_max: u64 = 0;
            let mut proc_stepped = 0;
            let mut proc_error: Option<DebuggerError> = None;

            for (hart_id, finished) in &hart_info {
                if *finished {
                    continue;
                }
                match self.step_hart_internal(*hart_id) {
                    Ok(cycles) => {
                        proc_stepped += 1;
                        proc_max = proc_max.max(cycles);
                        per_proc_ok_cycles[proc_idx].push((*hart_id, cycles));
                    }
                    Err(e) => {
                        log::debug!(
                            "step_all_harts: hart {} error (may be finished): {:?}",
                            hart_id,
                            e
                        );
                        if let Some(hart) = self.get_hart_mut(*hart_id) {
                            hart.exec_cycles = hart.exec_cycles.wrapping_add(1);
                        }
                        proc_error = Some(e);
                    }
                }
                // Track harts that just finished this round so Pass 2 can
                // correctly advance their finish_clock after the clock step.
                if let Some(hart) = self.get_hart(*hart_id) {
                    if matches!(hart.state, HartState::Finished) {
                        per_proc_finished_this_round[proc_idx].push(*hart_id);
                    }
                }
            }

            if proc_stepped > 0 {
                any_stepped = true;
                global_max_cycles = global_max_cycles.max(proc_max);
                total_stepped += proc_stepped;
            } else if proc_error.is_some() {
                // All harts in this processor failed — still count 1 fetch cycle
                global_max_cycles = global_max_cycles.max(1);
                errors.push(format!(
                    "processor {}: {:?}",
                    proc_idx,
                    proc_error.unwrap()
                ));
            }
        }

        // ── Pass 2: advance ALL processors by the same global_max_cycles ──
        // This is the key invariant: every processor always sees the same clock.
        if any_stepped || !errors.is_empty() {
            for proc_idx in 0..num_processors {
                let new_clock = self.processors[proc_idx]
                    .clock()
                    .wrapping_add(global_max_cycles);
                self.processors[proc_idx].set_clock(new_clock);

                // All harts in this processor age by the global cycle count
                for hart in &mut self.processors[proc_idx].harts {
                    hart.elapsed_cycles =
                        hart.elapsed_cycles.wrapping_add(global_max_cycles);
                }

                // Successfully-executing harts also get their individual exec_cycles
                for &(hart_id, cycles) in &per_proc_ok_cycles[proc_idx] {
                    if let Some(hart) = self.get_hart_mut(hart_id) {
                        hart.exec_cycles = hart.exec_cycles.wrapping_add(cycles);
                    }
                }

                // Fix finish_clock for harts that were finished during Pass 1:
                // finish_hart was called before the clock advance, so finish_clock
                // recorded the pre-advance clock. Advance it to the new clock so the
                // terminating instruction's cycle cost is included.
                // We use an explicit tracking list (per_proc_finished_this_round)
                // rather than a heuristic based on finish_clock value, because
                // finish_clock == old_clock would incorrectly match harts that
                // finished in previous rounds (whose finish_clock was already
                // corrected to what is now old_clock).
                for &hart_id in &per_proc_finished_this_round[proc_idx] {
                    if let Some(hart) = self.get_hart_mut(hart_id) {
                        hart.finish_clock = Some(new_clock);
                    }
                }
            }
        }

        if total_stepped > 0 {
            if !errors.is_empty() {
                log::warn!("step_all_harts: errors in some processors: {}", errors.join("; "));
            }
            Ok(total_stepped)
        } else if !errors.is_empty() {
            let msg = if errors.len() == 1 {
                errors.into_iter().next().unwrap()
            } else {
                errors.join("; ")
            };
            Err(DebuggerError::GeneralError(msg))
        } else {
            Ok(0)
        }
    }

    /// Add `count` additional harts to processor 0, each sharing the same
    /// program and memory but with independent register files and PCs.
    /// Each hart gets its globally unique ID and is initialized to the entry point.
    pub fn add_more_harts(&mut self, count: usize) -> Result<(), DebuggerError> {
        self.add_more_harts_to_processor(0, count)
    }

    /// Add `count` additional harts to a specific processor, each sharing
    /// the same program and memory but with independent register files and PCs.
    pub fn add_more_harts_to_processor(
        &mut self,
        proc_idx: usize,
        count: usize,
    ) -> Result<(), DebuggerError> {
        if proc_idx >= self.processors.len() {
            return Err(DebuggerError::GeneralError(format!(
                "proc_idx {} out of range ({} processors)", proc_idx, self.processors.len()
            )));
        }

        if self.programs.is_empty() {
            return Err(DebuggerError::GeneralError("no program loaded".into()));
        }

        let program_id = self.processors[proc_idx]
            .harts
            .first()
            .map(|h| h.program_id)
            .unwrap_or(0);

        if program_id >= self.programs.len() {
            return Err(DebuggerError::GeneralError(format!(
                "program_id {} out of range ({} programs)", program_id, self.programs.len()
            )));
        }

        let entry_point = self.programs[program_id]
            .get_entry_address2()
            .map_err(|x| DebuggerError::GeneralError(x.get_error_message()))?;

        let proc_clock = self.processors[proc_idx].clock();

        for _ in 0..count {
            let id = self.next_hart_id;
            self.next_hart_id += 1;
            let mut hart = Hart::new(id, program_id);
            hart.pc = entry_point;
            hart.csr.mhartid = id;
            hart.start_clock = proc_clock;
            // P0-4: Align elapsed_cycles to current processor clock so the new
            // hart shares the same time baseline as all existing harts. Without
            // this, step_all_harts creates a permanent gap between the new and
            // existing harts' elapsed_cycles.
            hart.elapsed_cycles = proc_clock;
            hart.state = HartState::Running;
            self.processors[proc_idx].harts.push(hart);
        }

        Ok(())
    }

    pub fn init_hart_to_entry_point(&mut self, hart_id: HartId) -> Result<(), DebuggerError> {
        let err_msg = format!("invalid hart: {}", hart_id);
        let program_id = {
            let hart = self.get_hart_mut(hart_id)
                                    .ok_or_else(|| DebuggerError::GeneralError(err_msg))?;
            hart.program_id
        };
        if program_id >= self.programs.len() {
            return Err(DebuggerError::GeneralError(
                format!("program_id {} out of range ({} programs)", program_id, self.programs.len())
            ));
        }
        let entry_point = self.programs[program_id]
                                                    .get_entry_address2()
                                                    .map_err(|x| DebuggerError::GeneralError(x.get_error_message()))?;
        // Capture processor clock before taking the mutable hart borrow
        let proc_idx = self.get_processor_index(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        let clock = self.processors[proc_idx].clock();
        let hart = self.get_hart_mut(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        // Reset all register files to default for clean restart, but preserve
        // mhartid — the hardware thread ID must remain immutable across restarts
        // per RISC-V spec. Resetting it to 0 would break multi-hart programs.
        let saved_mhartid = hart.csr.mhartid;
        hart.x = crate::machine::hart::IntegerRegisterFile::default();
        hart.f = crate::machine::hart::FloatRegisterFile::default();
        hart.v = crate::machine::hart::VectorRegisterFile::default();
        hart.csr = crate::machine::hart::CsrFile::default();
        hart.csr.mhartid = saved_mhartid;
        hart.error_info = None;
        hart.finish_clock = None;
        hart.pc = entry_point;
        // P0-4: Align elapsed_cycles to the current processor clock, NOT zero.
        // The architectural invariant requires all harts in the same processor
        // to share the same elapsed_cycles baseline.  Resetting to 0 creates a
        // permanent gap that destroys performance statistic accuracy.
        hart.elapsed_cycles = clock;
        hart.exec_cycles = 0;
        hart.inst_count = 0;
        hart.start_clock = clock;
        hart.state = HartState::Running;
        Ok(())
    }

    /// fetch instruction for given hart, O(1) lookup via cache
    fn fetch_inst(&self, hart: &Hart) -> Option<&Instruction> {
        if hart.program_id >= self.programs.len() {
            return None;
        }
        let (idx, _count) = self.inst_cache.get(&(hart.program_id, hart.pc))?;
        let program = &self.programs[hart.program_id];
        program.get_text_section_items()[*idx].get_inc()
    }

    /// get instruction offset for given hart, return None if no instruction found (e.g. pc is out of range)
    fn get_inst_offset(&self, hart: &Hart) -> Option<usize> {
        if hart.program_id >= self.programs.len() {
            return None;
        }
        let program = &self.programs[hart.program_id];
        let item = program
            .get_text_section_items()
            .into_iter()
            .find(|x| x.get_offset() == hart.pc);
        item.map(|x| x.get_offset())
    }

    /// get current instruction target address for given hart, 
    /// return None if no instruction found (e.g. pc is out of range)
    fn get_inst_target(&self, hart_id: HartId) -> Option<usize> {
        let hart = self.get_hart(hart_id)?;
        let inst = self.fetch_inst(hart)?;
        Some(inst.get_virtual_address() as usize)
    }

    /// check if there is an instruction at current pc of given hart
    pub fn has_inst_at_pc(&self, hart_id: HartId) -> bool {
        let hart = self.get_hart(hart_id);
        if let Some(hart) = hart {
            self.fetch_inst(hart).is_some()
        } else {
            false
        }
    }

    /// Step a hart forward by one instruction (debug mode).
    /// Advances ALL processors' clocks to maintain the "all processors share
    /// the same clock" invariant.  Corrects finish_clock ONLY for harts that
    /// just transitioned to Finished during THIS step (same precision as
    /// step_all_harts Pass 2).
    pub fn step_hart(&mut self, hart_id: HartId) -> Result<(), DebuggerError> {
        // Silently skip already-finished harts — consistent with step_all_harts
        // which also silently skips Finished harts.  Callers should check
        // hart.state == HartState::Finished if they need to distinguish.
        if let Some(hart) = self.get_hart(hart_id) {
            if matches!(hart.state, HartState::Finished) {
                return Ok(());
            }
        }
        let proc_idx = self.get_processor_index(hart_id);
        // P0-6/P0-10: Track whether the stepped hart was already finished BEFORE
        // the step, so after the clock advance we only update finish_clock for
        // harts that *just* transitioned — not all previously-finished harts.
        let was_finished_before = self
            .get_hart(hart_id)
            .map(|h| matches!(h.state, HartState::Finished))
            .unwrap_or(true);
        let result = self.step_hart_internal(hart_id);

        match result {
            Ok(cycles) => {
                // P0-22: Advance ALL processors by the same cycle count so the
                // "all processors share the same clock" invariant is maintained.
                // The previous code only advanced the current hart's processor,
                // creating a permanent clock skew between processors after the
                // first single-step.
                for proc in &mut self.processors {
                    let new_clock = proc.clock().wrapping_add(cycles);
                    proc.set_clock(new_clock);
                    for hart in &mut proc.harts {
                        hart.elapsed_cycles = hart.elapsed_cycles.wrapping_add(cycles);
                    }
                }
                // Update the stepped hart's exec_cycles
                if let Some(pi) = proc_idx {
                    if let Some(hart) = self.processors[pi]
                        .harts
                        .iter_mut()
                        .find(|h| h.id == hart_id)
                    {
                        hart.exec_cycles = hart.exec_cycles.wrapping_add(cycles);
                    }
                }
                // P0-6/P0-10: Correct finish_clock ONLY for the hart that was
                // stepped, and ONLY if it just transitioned to Finished during
                // THIS step.  Previously-finished harts must NOT have their
                // finish_clock pushed forward — doing so inflates runtime_cycles.
                // This mirrors step_all_harts' use of per_proc_finished_this_round.
                // Pre-compute the new clock before the mutable borrow on self
                // (get_hart_mut) to avoid a borrow conflict with the immutable
                // borrows needed to read the processor clock.
                if !was_finished_before {
                    let new_clock = self
                        .get_processor_index(hart_id)
                        .and_then(|pi| self.processors.get(pi).map(|p| p.clock()))
                        .unwrap_or(cycles);
                    if let Some(hart) = self.get_hart_mut(hart_id) {
                        if matches!(hart.state, HartState::Finished) {
                            hart.finish_clock = Some(new_clock);
                        }
                    }
                }
                Ok(())
            }
            Err(e) => {
                // Even on failure, fetch/decode consumed at least 1 cycle.
                // Advance ALL processors to maintain clock invariant.
                for proc in &mut self.processors {
                    let new_clock = proc.clock().wrapping_add(1);
                    proc.set_clock(new_clock);
                    for hart in &mut proc.harts {
                        hart.elapsed_cycles = hart.elapsed_cycles.wrapping_add(1);
                    }
                }
                if let Some(pi) = proc_idx {
                    if let Some(hart) = self.processors[pi]
                        .harts
                        .iter_mut()
                        .find(|h| h.id == hart_id)
                    {
                        hart.exec_cycles = hart.exec_cycles.wrapping_add(1);
                    }
                }
                // P0-6/P0-10: Only update finish_clock for the stepped hart if
                // it just transitioned to Finished on this error path.
                // Pre-compute new_clock to avoid borrow conflict (same as above).
                if !was_finished_before {
                    let new_clock = self
                        .get_processor_index(hart_id)
                        .and_then(|pi| self.processors.get(pi).map(|p| p.clock()))
                        .unwrap_or(1);
                    if let Some(hart) = self.get_hart_mut(hart_id) {
                        if matches!(hart.state, HartState::Finished) {
                            hart.finish_clock = Some(new_clock);
                        }
                    }
                }
                Err(e)
            }
        }
    }

    /// Mark a hart as Finished and record the processor clock at finish time.
    /// This freezes runtime_cycles at the actual execution duration, preventing
    /// it from growing indefinitely after program termination.
    fn finish_hart(&mut self, hart_id: HartId, error_info: Option<String>) {
        let finish_clock = self
            .get_processor_index(hart_id)
            .and_then(|pi| self.processors.get(pi).map(|p| p.clock()));
        if let Some(hart) = self.get_hart_mut(hart_id) {
            hart.state = HartState::Finished;
            hart.finish_clock = finish_clock;
            hart.error_info = error_info;
        }
    }

    /// Execute one instruction on the given hart.
    /// Updates hart.inst_count. Does NOT update hart.exec_cycles, elapsed_cycles, or
    /// global_clock — those are the caller's responsibility according to
    /// the clock model (shared-clock vs. debug-step).
    /// Returns the cycle cost of the executed instruction.
    fn step_hart_internal(&mut self, hart_id: HartId) -> Result<u64, DebuggerError> {

        let (program_id, pc) = {
            let hart = self.get_hart(hart_id)
                                    .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;

            (hart.program_id, hart.pc)
        };

        // Guard against default hart with no program loaded (empty programs list)
        if program_id >= self.programs.len() {
            let total_programs = self.programs.len();
            self.finish_hart(hart_id, Some(format!(
                "invalid program_id {} (only {} programs registered)",
                program_id, total_programs
            )));
            return Err(DebuggerError::GeneralError(format!(
                "hart {} has invalid program_id {} (no program loaded)", hart_id, program_id
            )));
        }

        // O(1) lookup via cache instead of linear scan.
        // Also retrieve machine_code_count so we can advance PC past all
        // machine-code words of an expanded pseudo-instruction.
        let cache_entry = self.inst_cache.get(&(program_id, pc)).copied();
        let inst = cache_entry.and_then(|(idx, _count)| {
            let prog = &self.programs[program_id];
            prog.get_text_section_items()[idx].get_inc().cloned()
        });
        let machine_code_count = cache_entry.map(|(_, count)| count);

        if let Some(inst) = inst {
            // Per-instruction detail log at debug level — avoids flooding during Continue
            let opcode = inst.get_op_code().map(|o| format!("{:?}", o)).unwrap_or_else(|_| "unknown".into());
            log::debug!("EXEC: pc={:x} op={} mnemonic={:?} r0={:?} r1={:?} r2={:?} imm={:?}",
                pc, opcode, inst.get_name(), inst.get_r0(), inst.get_r1(), inst.get_r2(), inst.get_imm());

            // Resolve cycle cost before execute_inst for per-opcode granularity
            let cycles = inst.get_op_code()
                .map(|o| Self::inst_cycles(&o))
                .unwrap_or(1);

            let result = self.execute_inst(hart_id, &inst);

            // Per-instruction register log at debug level — avoids flooding during Continue
            if let Some(hart) = self.get_hart(hart_id) {
                log::debug!("POST: t1(x6)={:x} t2(x7)={:x} t0(x5)={:x} a0(x10)={:x} a1(x11)={:x} a2(x12)={:x} a3(x13)={:x}",
                    hart.x.read(6), hart.x.read(7), hart.x.read(5),
                    hart.x.read(10), hart.x.read(11), hart.x.read(12),
                    hart.x.read(13));
            }

            // Accumulate inst count when execution succeeded
            if result.is_ok() {
                if let Some(hart) = self.get_hart_mut(hart_id) {
                    hart.inst_count = hart.inst_count.wrapping_add(1);
                }
                // Advance PC past any remaining machine-code words for this
                // text_section_item.  execute_inst already called next_pc once
                // (advancing by PC_INCREMENT) for non-branch instructions, so
                // we only need to advance for additional machine codes
                // (count - 1).  This is necessary because pseudo-instructions
                // like `la` can expand to multiple machine-code words, and the
                // PC must land on the next real instruction, not on an
                // intermediate offset within the expansion.
                //
                // P0-19: For branch/jump instructions (Jal, Jalr, Beq, Bne,
                // Blt, Bge, Bltu, Bgeu), execute_inst calls branch_to_target
                // which sets PC directly to the jump target.  Applying
                // extra_steps on top of that would advance PC past the correct
                // target and cause misaligned execution.  Skip extra_steps for
                // flow-control instructions.
                let is_flow_ctrl = inst.get_op_code().map(|op| {
                    matches!(op, OpCode::Jal | OpCode::Jalr
                        | OpCode::Beq | OpCode::Bne
                        | OpCode::Blt | OpCode::Bge
                        | OpCode::Bltu | OpCode::Bgeu)
                }).unwrap_or(false);
                let extra_steps = if is_flow_ctrl {
                    0
                } else {
                    machine_code_count.unwrap_or(1).saturating_sub(1)
                };
                if extra_steps > 0 {
                    if let Some(hart) = self.get_hart_mut(hart_id) {
                        for _ in 0..extra_steps {
                            hart.next_pc();
                        }
                    }
                }
                // Check if new PC is still within valid instruction range after
                // execution (branch/jump may have landed outside text section).
                // Mark Finished immediately so callers see the correct state
                // without needing an extra failed-fetch round.
                // P0-13: Only finish if not already Finished — branch_to_target
                // may have already called finish_hart with a specific error_info
                // (e.g. "branch/jump target address resolution failed"), and
                // calling finish_hart(None) here would overwrite that error.
                let pc_after = self.get_pc(hart_id)?;
                if !self.inst_cache.contains_key(&(program_id, pc_after)) {
                    let already_finished = self
                        .get_hart(hart_id)
                        .map(|h| matches!(h.state, HartState::Finished))
                        .unwrap_or(false);
                    if !already_finished {
                        self.finish_hart(hart_id, None);
                    }
                }
                return Ok(cycles);
            }

            // Mark hart as Finished when instruction execution fails (illegal
            // instruction, memory access error, etc.) so callers can distinguish
            // a hart that hit a fatal error from one still Running.
            let err_msg = format!("{:?}", result.as_ref().err());
            self.finish_hart(hart_id, Some(err_msg));

            result.map(|_| cycles)
        }
        else {
            // PC ran past last instruction — normal program exit
            self.finish_hart(hart_id, None);

            if self.programs[program_id].get_text_section_items().is_empty() {
                return Err(DebuggerError::GeneralError(format!("no instructions found in program id: {}", program_id)));
            }

            Err(DebuggerError::GeneralError(format!("no instruction found at pc: {}", pc)))
        }
    }

    /// Resolve register name to index, returning error instead of panicking
    fn resolve_reg_index(&self, reg: Option<&String>) -> Result<usize, DebuggerError> {
        self.registers
            .get_register_value(reg)
            .map(|v| v as usize)
            .map_err(|_| DebuggerError::RegisterReadError(
                format!("unknown register: {:?}", reg)
            ))
    }

    fn xreg(&self, name: &Option<String>) -> Result<usize, DebuggerError> {
        self.resolve_reg_index(name.as_ref())
    }

    fn get_f32(&self, hart_id: HartId, reg: Option<&String>) -> Result<f32, DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        // Read raw u64 bits, interpret lower 32 bits as f32 (RISC-V single-precision)
        Ok(f32::from_bits(hart.f.read(idx) as u32))
    }

    fn set_f32(&mut self, hart_id: HartId, reg: Option<&String>, value: f32) -> Result<(), DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart_mut(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        // NaN-box: upper 32 bits = all-1s, lower 32 bits = f32 bits
        let bits: u64 = 0xFFFFFFFF_00000000u64 | (value.to_bits() as u64);
        hart.f.write(idx, bits);
        Ok(())
    }

    fn get_f(&self, hart_id: HartId, reg: Option<&String>) -> Result<f64, DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        // Read raw u64 bits, interpret as f64 (RISC-V double-precision)
        Ok(f64::from_bits(hart.f.read(idx)))
    }

    pub fn get_f_bits(&self, hart_id: HartId, reg: Option<&String>) -> Result<u64, DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        Ok(hart.f.read(idx))
    }

    pub fn set_f_bits(&mut self, hart_id: HartId, reg: Option<&String>, bits: u64) -> Result<(), DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart_mut(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        hart.f.write(idx, bits);
        Ok(())
    }

    fn set_f(&mut self, hart_id: HartId, reg: Option<&String>, value: f64) -> Result<(), DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart_mut(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        hart.f.write(idx, value.to_bits());
        Ok(())
    }

    fn get_x(&self, hart_id: HartId, reg: Option<&String>) -> Result<u64, DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        Ok(hart.x.read(idx))
    }

    fn set_x(&mut self, hart_id: HartId, reg: Option<&String>, value: u64) -> Result<(), DebuggerError> {
        let idx = self.resolve_reg_index(reg)?;
        let hart = self.get_hart_mut(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;
        hart.x.write(idx, value); // IntegerRegisterFile::write guards x0
        Ok(())
    }

    /// get u64 from imm
    fn get_resolved_u64(&self, hart_id: HartId, inst: &Instruction) -> Result<u64, DebuggerError> {
        if inst.get_rel_fun().is_some() {
            Ok(inst.get_virtual_address() as u64)
        } else {
            self.get_u64_from_imm(hart_id, inst.get_imm())
        }
    }

    fn get_resolved_i64(&self, hart_id: HartId, inst: &Instruction) -> Result<i64, DebuggerError> {
        if inst.get_rel_fun().is_some() {
            Ok(inst.get_virtual_address() as u64 as i64)
        } else {
            self.get_i64_from_imm(hart_id, inst.get_imm())
        }
    }

    /// Return the cycle cost for a given RISC-V opcode.
    /// Typical simple ALU → 1, load → 2, mul → 3, float → 3, branch/jump → 2.
    fn inst_cycles(opcode: &OpCode) -> u64 {
        match opcode {
            // Integer ALU — 1 cycle
            OpCode::Add | OpCode::Sub | OpCode::And | OpCode::Or | OpCode::Xor
            | OpCode::Sll | OpCode::Srl | OpCode::Sra | OpCode::Slt | OpCode::Sltu => 1,
            // Integer immediate — 1 cycle
            OpCode::Addi | OpCode::Andi | OpCode::Ori | OpCode::Xori
            | OpCode::Slti | OpCode::Sltiu | OpCode::Slli | OpCode::Srli | OpCode::Srai
            | OpCode::Lui | OpCode::Auipc => 1,
            // Integer multiply — 3 cycles
            OpCode::Mul => 3,
            // Integer loads — 2 cycles
            OpCode::Lb | OpCode::Lbu | OpCode::Lh | OpCode::Lhu
            | OpCode::Lw | OpCode::Lwu | OpCode::Ld => 2,
            // Integer stores — 1 cycle
            OpCode::Sb | OpCode::Sh | OpCode::Sw | OpCode::Sd => 1,
            // Branches — 1 cycle base (not modelling taken penalty here for simplicity)
            OpCode::Beq | OpCode::Bne | OpCode::Blt | OpCode::Bge
            | OpCode::Bltu | OpCode::Bgeu => 1,
            // Jumps — 2 cycles
            OpCode::Jal | OpCode::Jalr => 2,
            // Double-precision float ALU — 3 cycles
            OpCode::Faddd | OpCode::Fsubd | OpCode::Fmuld | OpCode::Fdivd => 3,
            // Double-precision float compare — 1 cycle
            OpCode::Feqd | OpCode::Fltd | OpCode::Fled => 1,
            // Float move — 1 cycle
            OpCode::Fmvdx | OpCode::Fmvxd => 1,
            // Double-precision float load/store — 2 cycles
            OpCode::Fld | OpCode::Fsd => 2,
            // Single-precision float ALU — 3 cycles
            OpCode::Fadds | OpCode::Fsubs | OpCode::Fmuls | OpCode::Fdivs => 3,
            // Single-precision float compare — 1 cycle
            OpCode::Feqs | OpCode::Flts | OpCode::Fles => 1,
            // Default: 1 cycle for any unrecognized opcode
            _ => 1,
        }
    }

    fn binary_operation(&mut self, hart_id: HartId, inst: &Instruction, op: impl Fn(u64, u64) -> u64) -> Result<(), DebuggerError> {
        let lhs = self.get_x(hart_id, inst.get_r1())?;
        let rhs = self.get_x(hart_id, inst.get_r2())?;
        self.set_x(hart_id, inst.get_r0(), op(lhs, rhs))?;
        self.next_pc(hart_id)
    }

    /// Execute a branch/jump to the target resolved from the current instruction.
    /// The branch/jump instruction itself is always considered successfully executed
    /// (consistent with Jalr). If the target cannot be resolved, the hart is marked
    /// Finished to prevent an infinite loop — the post-execution PC validity check
    /// in step_hart_internal also handles invalid landing addresses.
    fn branch_to_target(&mut self, hart_id: HartId) -> Result<(), DebuggerError> {
        match self.get_inst_target(hart_id) {
            Some(target) => self.set_pc(hart_id, Some(target)),
            None => {
                // Target resolution failed (instruction metadata missing).
                // Mark Finished so we don't loop forever, but return Ok so
                // inst_count is incremented — the instruction itself executed.
                self.finish_hart(hart_id, Some("branch/jump target address resolution failed".into()));
                Ok(())
            }
        }
    }

    // ============================================================
    // Vector instruction helpers
    // ============================================================

    /// Get vector register index from register name
    fn vreg(&self, name: &Option<String>) -> usize {
        self.registers
            .get_register_value(name.as_ref())
            .unwrap() as usize
    }

    /// Read vector register bytes
    fn get_vreg_bytes(&self, hart_id: HartId, reg: Option<&String>) -> Vec<u8> {
        let idx = self.vreg(&reg.cloned());
        let hart = self.get_hart(hart_id).unwrap();
        hart.v.regs[idx].bytes.clone()
    }

    /// Write vector register bytes
    fn set_vreg_bytes(&mut self, hart_id: HartId, reg: Option<&String>, bytes: Vec<u8>) {
        let idx = self.vreg(&reg.cloned());
        let hart = self.get_hart_mut(hart_id).unwrap();
        if idx != 0 {
            hart.v.regs[idx].bytes = bytes;
        }
    }

    /// Read vector register bytes by index (for segment load/store)
    fn get_vreg_bytes_by_idx(&self, hart_id: HartId, idx: usize) -> Vec<u8> {
        let hart = self.get_hart(hart_id).unwrap();
        if idx < hart.v.regs.len() {
            hart.v.regs[idx].bytes.clone()
        } else {
            vec![0u8; 64]
        }
    }

    /// Write vector register bytes by index (for segment load/store)
    fn set_vreg_bytes_by_idx(&mut self, hart_id: HartId, idx: usize, bytes: Vec<u8>) {
        let hart = self.get_hart_mut(hart_id).unwrap();
        if idx != 0 && idx < hart.v.regs.len() {
            hart.v.regs[idx].bytes = bytes;
        }
    }

    /// Get SEW (selected element width) in bytes from vtype
    fn get_sew_bytes(&self, hart_id: HartId) -> usize {
        let hart = self.get_hart(hart_id).unwrap();
        let vtype = hart.csr.vtype;
        let sew_enc = ((vtype >> 3) & 0x7) as u8;
        match sew_enc {
            0b000 => 1,  // e8
            0b001 => 2,  // e16
            0b010 => 4,  // e32
            0b011 => 8,  // e64
            0b100 => 16, // e128
            0b101 => 32, // e256
            0b110 => 64, // e512
            0b111 => 128,// e1024
            _ => 4,
        }
    }

    /// Get VL (vector length) from CSR
    fn get_vl(&self, hart_id: HartId) -> usize {
        let hart = self.get_hart(hart_id).unwrap();
        hart.csr.vl as usize
    }

    /// Get effective element width: explicit width from name takes priority over SEW
    fn get_elem_width(&self, hart_id: HartId, name: &str) -> usize {
        Instruction::get_elem_width_from_name(name).unwrap_or_else(|| self.get_sew_bytes(hart_id))
    }

    /// Check if instruction is masked (option is "v0.t")
    fn is_masked(inst: &Instruction) -> bool {
        match inst.get_instruction_option() {
            Some(s) => s.to_lowercase() == "v0.t",
            None => false,
        }
    }

    /// Execute vector instruction dispatch
    fn execute_vector_inst(&mut self, hart_id: HartId, inst: &Instruction) -> Result<(), DebuggerError> {
        let name = inst.get_instruction_name().to_lowercase();

        // vsetvl/vsetvli/vsetivli - vector config
        if name.starts_with("vsetvl") || name.starts_with("vsetvli") || name.starts_with("vsetivli") {
            return self.execute_vset(hart_id, inst);
        }

        // vle* / vlse* / vluxei* / vloxei* / vl*re* - vector loads
        if name.starts_with("vle") || name.starts_with("vlse") || name.starts_with("vluxei") || name.starts_with("vloxei")
            || name.starts_with("vl1r") || name.starts_with("vl2r") || name.starts_with("vl3r") || name.starts_with("vl4r")
            || name.starts_with("vl1re") || name.starts_with("vl2re") || name.starts_with("vl3re") || name.starts_with("vl4re")
        {
            return self.execute_vload(hart_id, inst);
        }

        // vse* / vsse* / vsuxei* / vsoxei* / vs*r - vector stores
        if name.starts_with("vse") || name.starts_with("vsse") || name.starts_with("vsuxei") || name.starts_with("vsoxei")
            || name.starts_with("vs1r") || name.starts_with("vs2r") || name.starts_with("vs3r") || name.starts_with("vs4r")
        {
            return self.execute_vstore(hart_id, inst);
        }

        // vred*.vs - reductions
        if name.contains(".vs") && name.starts_with("vred") {
            return self.execute_vreduction(hart_id, inst);
        }

        // vand.mm / vor.mm / vxor.mm / vnot.m - mask instructions
        if (name.ends_with(".mm") || name.ends_with(".m")) && !name.starts_with("vms") {
            return self.execute_vmask(hart_id, inst);
        }

        // v*.vv / v*.vx / v*.vi - vector value instructions
        // Also handle .v suffix (e.g., vmv.v.v, vredsum.vs is handled above)
        if name.ends_with(".vv") || name.ends_with(".vx") || name.ends_with(".vi") || name.ends_with(".v.v") {
            return self.execute_vvalue(hart_id, inst);
        }

        Err(DebuggerError::GeneralError(format!("unsupported vector instruction: {}", name)))
    }

    /// Execute vsetvl/vsetvli/vsetivli
    fn execute_vset(&mut self, hart_id: HartId, inst: &Instruction) -> Result<(), DebuggerError> {
        let name = inst.get_instruction_name().to_lowercase();

        let (avl, vtypei) = if name.starts_with("vsetivli") {
            let avl = inst.get_imm()
                .and_then(|i| {
                    if let Imm::Value(s) = i {
                        core_utils::number::get_u64_from_str(s).ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0) as usize;
            let vtypei = inst.get_instruction_option()
                .and_then(|opt| Self::parse_vtypei(opt))
                .unwrap_or(0);
            (avl, vtypei)
        } else if name.starts_with("vsetvli") {
            let rs1 = inst.get_r1();
            let avl = self.get_x(hart_id, rs1)? as usize;
            let vtypei = inst.get_instruction_option()
                .and_then(|opt| Self::parse_vtypei(opt))
                .unwrap_or(0);
            (avl, vtypei)
        } else {
            // vsetvl rd, rs1, rs2
            let rs1 = inst.get_r1();
            let rs2 = inst.get_r2();
            let avl = self.get_x(hart_id, rs1)? as usize;
            let vtypei = self.get_x(hart_id, rs2)?;
            (avl, vtypei)
        };

        // Compute VLMAX = (VLEN / SEW) * LMUL
        // VLEN = 512 bits = 64 bytes (VectorRegister has 64 bytes)
        let vlen_bytes = 64;
        let sew_enc = ((vtypei >> 3) & 0x7) as usize;
        // SEW encoding: 0=e8(1byte), 1=e16(2bytes), 2=e32(4bytes), 3=e64(8bytes)
        let sew_bytes = 1 << sew_enc;
        let lmul_enc = (vtypei & 0x7) as usize;
        let lmul = match lmul_enc {
            0 => 1,   // m1
            1 => 2,   // m2
            2 => 4,   // m4
            3 => 8,   // m8
            5 => 1,   // mf8 -> 1/8, but we treat as 1 for now (fractional LMUL)
            6 => 1,   // mf4 -> 1/4
            7 => 1,   // mf2 -> 1/2
            _ => 1,
        };
        let vlmax = (vlen_bytes / sew_bytes) * lmul;

        let vl = if avl > vlmax {
            vlmax
        } else {
            avl
        };

        {
            let hart = self.get_hart_mut(hart_id).unwrap();
            hart.csr.vl = vl as u64;
            hart.csr.vtype = vtypei;
            hart.vector_state.vl = vl;
            hart.vector_state.sew = sew_bytes;
            hart.vector_state.lmul = lmul;
            hart.csr.vlenb = vlen_bytes as u64;
        }

        self.set_x(hart_id, inst.get_r0(), vl as u64);
        self.next_pc(hart_id)?;
        Ok(())
    }

    /// Execute vector value instructions (VV/VX/VI forms)
    fn execute_vvalue(&mut self, hart_id: HartId, inst: &Instruction) -> Result<(), DebuggerError> {
        let name = inst.get_instruction_name().to_lowercase();
        let vl = self.get_vl(hart_id);
        let sew = self.get_elem_width(hart_id, &name);
        let masked = Self::is_masked(inst);

        if vl == 0 {
            self.next_pc(hart_id)?;
            return Ok(());
        }

        let is_vi = name.ends_with(".vi");
        let is_vx = name.ends_with(".vx");

        let base_name = name.split('.').next().unwrap_or(&name);
        let op_name = if base_name == "vmv" { "move" }
            else if base_name == "vmerge" { "merge" }
            else if base_name.starts_with("vmseq") { "seq" }
            else if base_name.starts_with("vmsltu") { "sltu" }
            else if base_name.starts_with("vmslt") { "slt" }
            else if base_name.starts_with("vmsle") { "sle" }
            else if base_name.starts_with("vmsgtu") { "sgtu" }
            else if let Some(stripped) = base_name.strip_prefix('v') { stripped }
            else { base_name };

        let vs2_bytes = self.get_vreg_bytes(hart_id, inst.get_r1());
        let vs1_bytes = if is_vx || is_vi {
            Vec::new()
        } else {
            self.get_vreg_bytes(hart_id, inst.get_r2())
        };

        let vs1_scalar = if is_vx || is_vi {
            Some(if is_vi {
                inst.get_imm()
                    .and_then(|i| {
                        if let Imm::Value(s) = i {
                            core_utils::number::get_i64_from_str(s).ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0) as u64
            } else {
                self.get_x(hart_id, inst.get_r2())?
            })
        } else {
            None
        };

        let vd_orig_bytes = self.get_vreg_bytes(hart_id, inst.get_r0());

        let is_macc = op_name == "madd" || op_name == "nmsub" || op_name == "macc" || op_name == "nmacc";

        let mut vd_bytes = if is_macc {
            let mut b = vec![0u8; 64];
            b[..vd_orig_bytes.len().min(64)].copy_from_slice(&vd_orig_bytes[..vd_orig_bytes.len().min(64)]);
            b
        } else if op_name == "merge" || op_name == "move" {
            let mut b = vec![0u8; 64];
            b[..vs2_bytes.len().min(64)].copy_from_slice(&vs2_bytes[..vs2_bytes.len().min(64)]);
            b
        } else {
            vec![0u8; 64]
        };

        for i in 0..vl {
            let byte_offset = i * sew;
            if byte_offset + sew > 64 { break; }

            if masked {
                let v0_bytes = self.get_vreg_bytes(hart_id, Some(&"v0".to_string()));
                let mask_byte = v0_bytes.get(byte_offset).copied().unwrap_or(0);
                if mask_byte & 1 == 0 { continue; }
            }

            let vs2_elem = Self::read_elem(&vs2_bytes, byte_offset, sew);
            let vs1_elem = if let Some(scalar) = vs1_scalar {
                scalar
            } else {
                Self::read_elem(&vs1_bytes, byte_offset, sew)
            };
            let vd_orig_elem = if is_macc {
                Self::read_elem(&vd_orig_bytes, byte_offset, sew)
            } else {
                0
            };

            let result = match op_name {
                "add" => vs2_elem.wrapping_add(vs1_elem),
                "sub" => vs2_elem.wrapping_sub(vs1_elem),
                "mul" => vs2_elem.wrapping_mul(vs1_elem),
                "div" => {
                    let a = Self::sign_extend(vs2_elem, sew);
                    let b = Self::sign_extend(vs1_elem, sew);
                    if b == 0 { u64::MAX } else if a == i64::MIN && b == -1 { a as u64 } else { a.wrapping_div(b) as u64 }
                }
                "divu" => {
                    if vs1_elem == 0 { u64::MAX } else { vs2_elem.wrapping_div(vs1_elem) }
                }
                "rem" => {
                    let a = Self::sign_extend(vs2_elem, sew);
                    let b = Self::sign_extend(vs1_elem, sew);
                    if b == 0 { a as u64 } else if a == i64::MIN && b == -1 { 0 } else { a.wrapping_rem(b) as u64 }
                }
                "remu" => {
                    if vs1_elem == 0 { vs2_elem } else { vs2_elem.wrapping_rem(vs1_elem) }
                }
                "min" => {
                    let a = Self::sign_extend(vs2_elem, sew);
                    let b = Self::sign_extend(vs1_elem, sew);
                    a.min(b) as u64
                }
                "minu" => vs2_elem.min(vs1_elem),
                "max" => {
                    let a = Self::sign_extend(vs2_elem, sew);
                    let b = Self::sign_extend(vs1_elem, sew);
                    a.max(b) as u64
                }
                "maxu" => vs2_elem.max(vs1_elem),
                "and" => vs2_elem & vs1_elem,
                "or" => vs2_elem | vs1_elem,
                "xor" => vs2_elem ^ vs1_elem,
                "sll" => {
                    let sew_bits = sew * 8;
                    let shamt_mask = (sew_bits - 1) as u64;
                    vs2_elem.wrapping_shl((vs1_elem & shamt_mask) as u32)
                }
                "srl" => {
                    let sew_bits = sew * 8;
                    let shamt_mask = (sew_bits - 1) as u64;
                    vs2_elem.wrapping_shr((vs1_elem & shamt_mask) as u32)
                }
                "sra" => {
                    let a = Self::sign_extend(vs2_elem, sew);
                    let sew_bits = sew * 8;
                    let shamt_mask = (sew_bits - 1) as u64;
                    let shift = (vs1_elem & shamt_mask) as u32;
                    if sew_bits >= 64 {
                        (a.wrapping_shr(shift)) as u64
                    } else {
                        ((a.wrapping_shr(shift)) as u64) & ((1u64 << sew_bits) - 1)
                    }
                }
                "seq" => if vs2_elem == vs1_elem { u64::MAX } else { 0 },
                "slt" => if Self::sign_extend(vs2_elem, sew) < Self::sign_extend(vs1_elem, sew) { u64::MAX } else { 0 },
                "sle" => if Self::sign_extend(vs2_elem, sew) <= Self::sign_extend(vs1_elem, sew) { u64::MAX } else { 0 },
                "sltu" => if vs2_elem < vs1_elem { u64::MAX } else { 0 },
                "sgtu" => if vs2_elem > vs1_elem { u64::MAX } else { 0 },
                "madd" => vd_orig_elem.wrapping_add(vs1_elem.wrapping_mul(vs2_elem)),
                "nmsub" => vs1_elem.wrapping_mul(vs2_elem).wrapping_sub(vd_orig_elem),
                "macc" => vd_orig_elem.wrapping_add(vs1_elem.wrapping_mul(vs2_elem)),
                "nmacc" => 0u64.wrapping_sub(vd_orig_elem.wrapping_add(vs1_elem.wrapping_mul(vs2_elem))),
                "merge" | "move" => vs1_elem,
                _ => return Err(DebuggerError::GeneralError(format!("unsupported vector op: {}", op_name))),
            };

            Self::write_elem(&mut vd_bytes, byte_offset, sew, result);
        }

        self.set_vreg_bytes(hart_id, inst.get_r0(), vd_bytes);
        self.next_pc(hart_id)?;
        Ok(())
    }

    /// Execute vector load instructions
    fn execute_vload(&mut self, hart_id: HartId, inst: &Instruction) -> Result<(), DebuggerError> {
        let name = inst.get_instruction_name().to_lowercase();
        let vl = self.get_vl(hart_id);
        let sew = self.get_elem_width(hart_id, &name);

        if vl == 0 {
            self.next_pc(hart_id)?;
            return Ok(());
        }

        let nfields = if name.contains("vl4r") { 4 }
            else if name.contains("vl3r") { 3 }
            else if name.contains("vl2r") { 2 }
            else { 1 };

        let vd_start_idx = self.vreg(&inst.get_r0().cloned());
        let base_addr = self.get_x(hart_id, inst.get_r1())?;

        let is_strided = name.starts_with("vlse");
        let is_indexed = name.starts_with("vluxei") || name.starts_with("vloxei");
        let is_segment_whole_reg = name.starts_with("vl1r") || name.starts_with("vl2r")
            || name.starts_with("vl3r") || name.starts_with("vl4r");

        let stride = if is_strided {
            self.get_x(hart_id, inst.get_r2())?
        } else {
            0
        };
        let vindex_bytes = if is_indexed {
            Some(self.get_vreg_bytes(hart_id, inst.get_r2()))
        } else {
            None
        };

        if is_segment_whole_reg {
            let bytes_per_reg = 64;
            for field in 0..nfields {
                let mut vd_bytes = vec![0u8; bytes_per_reg];
                for b in 0..bytes_per_reg {
                    let addr = base_addr.wrapping_add((field * bytes_per_reg + b) as u64);
                    vd_bytes[b] = self.memory.read_u8(addr)?;
                }
                let reg_idx = (vd_start_idx + field) % 32;
                self.set_vreg_bytes_by_idx(hart_id, reg_idx, vd_bytes);
            }
        } else {
            let mut vd_bytes = vec![0u8; 64];

            for i in 0..vl {
                let byte_offset = i * sew;
                if byte_offset + sew > 64 { break; }

                let addr = if is_indexed {
                    let vindex = vindex_bytes.as_ref().unwrap();
                    let idx_elem = Self::read_elem(vindex, i * sew, sew);
                    // RISC-V spec: indexed load/store uses the index value directly
                    // as a byte offset (both ordered and unordered variants)
                    base_addr.wrapping_add(idx_elem)
                } else if is_strided {
                    base_addr.wrapping_add((i as u64).wrapping_mul(stride))
                } else {
                    base_addr.wrapping_add((i * sew) as u64)
                };

                let val = match sew {
                    1 => self.memory.read_u8(addr)? as u64,
                    2 => self.memory.read_u16(addr)? as u64,
                    4 => self.memory.read_u32(addr)? as u64,
                    8 => self.memory.read_u64(addr)?,
                    _ => self.memory.read_u64(addr)?,
                };

                Self::write_elem(&mut vd_bytes, byte_offset, sew, val);
            }

            self.set_vreg_bytes(hart_id, inst.get_r0(), vd_bytes);
        }

        self.next_pc(hart_id)?;
        Ok(())
    }

    /// Execute vector store instructions
    fn execute_vstore(&mut self, hart_id: HartId, inst: &Instruction) -> Result<(), DebuggerError> {
        let name = inst.get_instruction_name().to_lowercase();
        let vl = self.get_vl(hart_id);
        let sew = self.get_elem_width(hart_id, &name);

        if vl == 0 {
            self.next_pc(hart_id)?;
            return Ok(());
        }

        let nfields = if name.contains("vs4r") { 4 }
            else if name.contains("vs3r") { 3 }
            else if name.contains("vs2r") { 2 }
            else { 1 };

        let vd_start_idx = self.vreg(&inst.get_r0().cloned());
        let base_addr = self.get_x(hart_id, inst.get_r1())?;

        let is_strided = name.starts_with("vsse");
        let is_indexed = name.starts_with("vsuxei") || name.starts_with("vsoxei");
        let is_segment_whole_reg = name.starts_with("vs1r") || name.starts_with("vs2r")
            || name.starts_with("vs3r") || name.starts_with("vs4r");

        let stride = if is_strided {
            self.get_x(hart_id, inst.get_r2())?
        } else {
            0
        };
        let vindex_bytes = if is_indexed {
            Some(self.get_vreg_bytes(hart_id, inst.get_r2()))
        } else {
            None
        };

        if is_segment_whole_reg {
            let bytes_per_reg = 64;
            for field in 0..nfields {
                let reg_idx = (vd_start_idx + field) % 32;
                let vs_bytes = self.get_vreg_bytes_by_idx(hart_id, reg_idx);
                for b in 0..bytes_per_reg {
                    let addr = base_addr.wrapping_add((field * bytes_per_reg + b) as u64);
                    self.memory.write_u8(addr, vs_bytes.get(b).copied().unwrap_or(0));
                }
            }
        } else {
            let vs3_bytes = self.get_vreg_bytes(hart_id, inst.get_r0());

            for i in 0..vl {
                let byte_offset = i * sew;
                if byte_offset + sew > 64 { break; }

                let addr = if is_indexed {
                    let vindex = vindex_bytes.as_ref().unwrap();
                    let idx_elem = Self::read_elem(vindex, i * sew, sew);
                    // RISC-V spec: indexed load/store uses the index value directly
                    // as a byte offset (both ordered and unordered variants)
                    base_addr.wrapping_add(idx_elem)
                } else if is_strided {
                    base_addr.wrapping_add((i as u64).wrapping_mul(stride))
                } else {
                    base_addr.wrapping_add((i * sew) as u64)
                };

                let val = Self::read_elem(&vs3_bytes, byte_offset, sew);
                match sew {
                    1 => self.memory.write_u8(addr, val as u8)?,
                    2 => self.memory.write_u16(addr, val as u16)?,
                    4 => self.memory.write_u32(addr, val as u32)?,
                    8 => self.memory.write_u64(addr, val)?,
                    _ => {}
                }
            }
        }

        self.next_pc(hart_id)?;
        Ok(())
    }

    /// Execute vector reduction instructions
    fn execute_vreduction(&mut self, hart_id: HartId, inst: &Instruction) -> Result<(), DebuggerError> {
        let name = inst.get_instruction_name().to_lowercase();
        let vl = self.get_vl(hart_id);
        let sew = self.get_elem_width(hart_id, &name);
        let masked = Self::is_masked(inst);

        if vl == 0 {
            self.next_pc(hart_id)?;
            return Ok(());
        }

        let base_name = name.split('.').next().unwrap_or(&name);
        let vs2_bytes = self.get_vreg_bytes(hart_id, inst.get_r1());
        let vs1_bytes = self.get_vreg_bytes(hart_id, inst.get_r2());

        let vs1_elem = Self::read_elem(&vs1_bytes, 0, sew);
        let mut result = vs1_elem;

        for i in 0..vl {
            let byte_offset = i * sew;
            if byte_offset + sew > 64 { break; }

            if masked {
                let v0_bytes = self.get_vreg_bytes(hart_id, Some(&"v0".to_string()));
                let mask_byte = v0_bytes.get(byte_offset).copied().unwrap_or(0);
                if mask_byte & 1 == 0 { continue; }
            }

            let elem = Self::read_elem(&vs2_bytes, byte_offset, sew);

            result = match base_name {
                "vredsum" => result.wrapping_add(elem),
                "vredmin" => {
                    let a = Self::sign_extend(result, sew);
                    let b = Self::sign_extend(elem, sew);
                    a.min(b) as u64
                }
                "vredmax" => {
                    let a = Self::sign_extend(result, sew);
                    let b = Self::sign_extend(elem, sew);
                    a.max(b) as u64
                }
                _ => result,
            };
        }

        let mut vd_bytes = vec![0u8; 64];
        Self::write_elem(&mut vd_bytes, 0, sew, result);
        self.set_vreg_bytes(hart_id, inst.get_r0(), vd_bytes);
        self.next_pc(hart_id)?;
        Ok(())
    }

    /// Parse vtypei from option string like "e32,m1,ta,ma"
    fn parse_vtypei(opt: &str) -> Option<u64> {
        let s = opt.to_lowercase().replace(' ', "");
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() < 2 { return None; }
        let sew = match parts[0] {
            "e8" => 0u64, "e16" => 1, "e32" => 2, "e64" => 3,
            "e128" => 4, "e256" => 5, "e512" => 6, "e1024" => 7,
            _ => return None,
        };
        let lmul = match parts[1] {
            "m1" => 0u64, "m2" => 1, "m4" => 2, "m8" => 3,
            "mf8" => 5, "mf4" => 6, "mf2" => 7,
            _ => return None,
        };
        let mut vta = 0u64;
        let mut vma = 0u64;
        for part in &parts[2..] {
            match *part {
                "ta" => vta = 1,
                "ma" => vma = 1,
                _ => {}
            }
        }
        Some((vma << 7) | (vta << 6) | (sew << 3) | lmul)
    }

    /// Execute vector mask instructions (vand.mm, vor.mm, vxor.mm, vnot.m)
    fn execute_vmask(&mut self, hart_id: HartId, inst: &Instruction) -> Result<(), DebuggerError> {
        let name = inst.get_instruction_name().to_lowercase();
        let vl = self.get_vl(hart_id);

        if vl == 0 {
            self.next_pc(hart_id)?;
            return Ok(());
        }

        let vs1_bytes = self.get_vreg_bytes(hart_id, inst.get_r1());
        let vs2_bytes = if name.starts_with("vnot") {
            None
        } else {
            Some(self.get_vreg_bytes(hart_id, inst.get_r2()))
        };

        let base_name = name.split('.').next().unwrap_or(&name);
        let mut vd_bytes = vec![0u8; 64];

        for i in 0..64 {
            let vs1_byte = vs1_bytes.get(i).copied().unwrap_or(0);
            let vs2_byte = vs2_bytes.as_ref().map(|v| v.get(i).copied().unwrap_or(0)).unwrap_or(0);

            let result_byte = match base_name {
                "vand" => vs1_byte & vs2_byte,
                "vor" => vs1_byte | vs2_byte,
                "vxor" => vs1_byte ^ vs2_byte,
                "vnot" => !vs1_byte,
                _ => vs1_byte,
            };

            if i < vd_bytes.len() {
                vd_bytes[i] = result_byte;
            }
        }

        self.set_vreg_bytes(hart_id, inst.get_r0(), vd_bytes);
        self.next_pc(hart_id)?;
        Ok(())
    }

    /// Read element from byte slice at given offset with given size
    fn read_elem(bytes: &[u8], offset: usize, size: usize) -> u64 {
        let mut val: u64 = 0;
        for b in 0..size {
            if offset + b < bytes.len() {
                val |= (bytes[offset + b] as u64) << (b * 8);
            }
        }
        val
    }

    /// Sign-extend a u64 value based on element size in bytes
    fn sign_extend(value: u64, sew_bytes: usize) -> i64 {
        if sew_bytes >= 8 {
            return value as i64;
        }
        let bits = (sew_bytes * 8) as u32;
        let sign_bit = 1u64 << (bits - 1);
        if value & sign_bit != 0 {
            (value | (u64::MAX << bits)) as i64
        } else {
            value as i64
        }
    }

    /// Write element to byte slice at given offset with given size
    fn write_elem(bytes: &mut [u8], offset: usize, size: usize, value: u64) {
        for b in 0..size {
            if offset + b < bytes.len() {
                bytes[offset + b] = ((value >> (b * 8)) & 0xFF) as u8;
            }
        }
    }

    fn execute_inst(&mut self, hart_id: HartId, inst: &Instruction) -> Result<(), DebuggerError> {
        // Check if this is a vector instruction (name starts with 'v')
        let name = inst.get_instruction_name();
        if name.to_lowercase().starts_with('v') {
            return self.execute_vector_inst(hart_id, inst);
        }

        let opcode = inst.get_op_code()
                    .map_err(|x| DebuggerError::GeneralError(x.get_error_message()))?;

        let proc_idx = self.get_processor_index(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;

        match opcode {
            OpCode::Add => self.binary_operation(hart_id, &inst, u64::wrapping_add)?,
            OpCode::Sub => self.binary_operation(hart_id, &inst, u64::wrapping_sub)?,
            OpCode::And => {
                // C.AND maps to And with r0=rd, r1=rs2, r2=None. Regular and uses r1,r2.
                if inst.get_r2().is_none() {
                    let lhs = self.get_x(hart_id, inst.get_r0())?;
                    let rhs = self.get_x(hart_id, inst.get_r1())?;
                    self.set_x(hart_id, inst.get_r0(), lhs & rhs);
                    self.next_pc(hart_id)?;
                } else {
                    self.binary_operation(hart_id, &inst, |a, b| a & b)?;
                }
            }
            OpCode::Or  => self.binary_operation(hart_id, &inst, |a, b| a | b)?,
            OpCode::Xor => self.binary_operation(hart_id, &inst, |a, b| a ^ b)?,
            OpCode::Sll => self.binary_operation(hart_id, &inst, |a, b| a << (b & 0x3f) as u32)?,
            OpCode::Srl => self.binary_operation(hart_id, &inst, |a, b| a >> (b & 0x3f) as u32)?,
            OpCode::Sra => self.binary_operation(hart_id, &inst, |a, b| ((a as i64) >> (b & 0x3f) as u32) as u64)?,
            OpCode::Slt  => self.binary_operation(hart_id, &inst, |a, b| ((a as i64) < (b as i64)) as u64)?,
            OpCode::Sltu => self.binary_operation(hart_id, &inst, |a, b| (a < b) as u64)?,
            OpCode::Mul  => self.binary_operation(hart_id, &inst, |a, b| a.wrapping_mul(b))?, 
            OpCode::Andn => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                self.set_x(hart_id, inst.get_r0(), lhs & !rhs);
                self.next_pc(hart_id)?;
            }
            OpCode::Orn => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                self.set_x(hart_id, inst.get_r0(), lhs | !rhs);
                self.next_pc(hart_id)?;
            }
            OpCode::Xnor => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                self.set_x(hart_id, inst.get_r0(), !(lhs ^ rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Rol => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                let shamt = (rhs & 0x3f) as u32;
                self.set_x(hart_id, inst.get_r0(), lhs.rotate_left(shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Ror => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                let shamt = (rhs & 0x3f) as u32;
                self.set_x(hart_id, inst.get_r0(), lhs.rotate_right(shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Rori => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let shamt = (self.get_resolved_u64(hart_id, inst)? & 0x3f) as u32;
                self.set_x(hart_id, inst.get_r0(), lhs.rotate_right(shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Clz => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), val.leading_zeros() as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Ctz => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), val.trailing_zeros() as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Cpop => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), val.count_ones() as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Sextb => {
                let val = self.get_x(hart_id, inst.get_r1())? as i8 as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Sexth => {
                let val = self.get_x(hart_id, inst.get_r1())? as i16 as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Zexth => {
                let val = self.get_x(hart_id, inst.get_r1())? as u16 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Orcb => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let mut result: u64 = 0;
                for i in 0..8 {
                    if (val >> (i * 8)) & 0xff != 0 {
                        result |= 0xffu64 << (i * 8);
                    }
                }
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Rev8 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let result = val.swap_bytes();
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Bclr => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                self.set_x(hart_id, inst.get_r0(), lhs & !(1u64 << (rhs & 0x3f)));
                self.next_pc(hart_id)?;
            }
            OpCode::Bclri => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let shamt = self.get_resolved_u64(hart_id, inst)? & 0x3f;
                self.set_x(hart_id, inst.get_r0(), lhs & !(1u64 << shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Bset => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                self.set_x(hart_id, inst.get_r0(), lhs | (1u64 << (rhs & 0x3f)));
                self.next_pc(hart_id)?;
            }
            OpCode::Bseti => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let shamt = self.get_resolved_u64(hart_id, inst)? & 0x3f;
                self.set_x(hart_id, inst.get_r0(), lhs | (1u64 << shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Bext => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                self.set_x(hart_id, inst.get_r0(), (lhs >> (rhs & 0x3f)) & 1);
                self.next_pc(hart_id)?;
            }
            OpCode::Bexti => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let shamt = self.get_resolved_u64(hart_id, inst)? & 0x3f;
                self.set_x(hart_id, inst.get_r0(), (lhs >> shamt) & 1);
                self.next_pc(hart_id)?;
            }
            OpCode::Binv => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                self.set_x(hart_id, inst.get_r0(), lhs ^ (1u64 << (rhs & 0x3f)));
                self.next_pc(hart_id)?;
            }
            OpCode::Binvi => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let shamt = self.get_resolved_u64(hart_id, inst)? & 0x3f;
                self.set_x(hart_id, inst.get_r0(), lhs ^ (1u64 << shamt));
                self.next_pc(hart_id)?;
            }
            OpCode::Min => {
                let lhs = self.get_x(hart_id, inst.get_r1())? as i64;
                let rhs = self.get_x(hart_id, inst.get_r2())? as i64;
                self.set_x(hart_id, inst.get_r0(), lhs.min(rhs) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Minu => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                self.set_x(hart_id, inst.get_r0(), lhs.min(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Max => {
                let lhs = self.get_x(hart_id, inst.get_r1())? as i64;
                let rhs = self.get_x(hart_id, inst.get_r2())? as i64;
                self.set_x(hart_id, inst.get_r0(), lhs.max(rhs) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Maxu => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                self.set_x(hart_id, inst.get_r0(), lhs.max(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Clzw => {
                let val = self.get_x(hart_id, inst.get_r1())? as u32;
                self.set_x(hart_id, inst.get_r0(), (val.leading_zeros() as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Ctzw => {
                let val = self.get_x(hart_id, inst.get_r1())? as u32;
                self.set_x(hart_id, inst.get_r0(), (val.trailing_zeros() as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Cpopw => {
                let val = self.get_x(hart_id, inst.get_r1())? as u32;
                self.set_x(hart_id, inst.get_r0(), (val.count_ones() as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Rolw => {
                let lhs = self.get_x(hart_id, inst.get_r1())? as u32;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                let shamt = (rhs & 0x1f) as u32;
                self.set_x(hart_id, inst.get_r0(), (lhs.rotate_left(shamt) as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Rorw => {
                let lhs = self.get_x(hart_id, inst.get_r1())? as u32;
                let rhs = self.get_x(hart_id, inst.get_r2())?;
                let shamt = (rhs & 0x1f) as u32;
                self.set_x(hart_id, inst.get_r0(), (lhs.rotate_right(shamt) as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Roriw => {
                let lhs = self.get_x(hart_id, inst.get_r1())? as u32;
                let shamt = (self.get_resolved_u64(hart_id, inst)? & 0x1f) as u32;
                self.set_x(hart_id, inst.get_r0(), (lhs.rotate_right(shamt) as i32 as i64) as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Addi => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs.wrapping_add_signed(imm),
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Andi => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs & imm,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Ori => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs | imm,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Xori => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs ^ imm,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Slti => {

                let lhs = self.get_x(hart_id, inst.get_r1())? as i64;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < imm { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Sltiu => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < imm { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Slli => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let shamt = (self.get_resolved_u64(hart_id, inst)? & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs << shamt,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Srli => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;
                let shamt = (self.get_resolved_u64(hart_id, inst)? & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    lhs >> shamt,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Srai => {
                let lhs = self.get_x(hart_id, inst.get_r1())? as i64;
                let shamt = (self.get_resolved_u64(hart_id, inst)? & 0x3f) as u32;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    (lhs >> shamt) as u64,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lui => {
                let imm = self.get_resolved_i64(hart_id, inst)?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    (imm << 12) as u64,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lb => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value =  self.processors[proc_idx].memory.read_i8(addr)? as i64 as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lbu => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);

                let value = self.processors[proc_idx].memory.read_u8(addr)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lh => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value =  self.processors[proc_idx].memory.read_i16(addr)? as i64 as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lhu => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value = self.processors[proc_idx].memory.read_u16(addr)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lw => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value =  self.processors[proc_idx].memory.read_i32(addr)? as i64 as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Lwu => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value = self.processors[proc_idx].memory.read_u32(addr)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Ld => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;

                let addr = base.wrapping_add_signed(imm);
                let value = self.processors[proc_idx].memory.read_u64(addr)?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Sb => {
                let value = self.get_x(hart_id, inst.get_r2())?;  // rs2 = value to store
                let base = self.get_x(hart_id, inst.get_r1())?;   // rs1 = base address

                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);

                self.processors[proc_idx].memory.write_u8(
                    addr,
                    value as u8,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Sh => {
                let value = self.get_x(hart_id, inst.get_r2())?;  // rs2 = value to store
                let base = self.get_x(hart_id, inst.get_r1())?;   // rs1 = base address

                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);

                self.processors[proc_idx].memory.write_u16(
                    addr,
                    value as u16,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Sw => {
                let value = self.get_x(hart_id, inst.get_r2())?;  // rs2 = value to store
                let base = self.get_x(hart_id, inst.get_r1())?;   // rs1 = base address

                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);

                self.processors[proc_idx].memory.write_u32(
                    addr,
                    value as u32,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Sd => {
                let value = self.get_x(hart_id, inst.get_r2())?;  // rs2 = value to store
                let base = self.get_x(hart_id, inst.get_r1())?;   // rs1 = base address

                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);

                self.processors[proc_idx].memory.write_u64(
                    addr,
                    value,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Beq => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;  // rs1
                let rhs = self.get_x(hart_id, inst.get_r2())?;  // rs2

                if lhs == rhs {
                    self.branch_to_target(hart_id)?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bne => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;  // rs1
                let rhs = self.get_x(hart_id, inst.get_r2())?;  // rs2

                if lhs != rhs {
                    self.branch_to_target(hart_id)?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Blt => {
                let lhs = self.get_x(hart_id, inst.get_r1())? as i64;  // rs1
                let rhs = self.get_x(hart_id, inst.get_r2())? as i64;  // rs2

                if lhs < rhs {
                    self.branch_to_target(hart_id)?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bge => {
                let lhs = self.get_x(hart_id, inst.get_r1())? as i64;  // rs1
                let rhs = self.get_x(hart_id, inst.get_r2())? as i64;  // rs2

                if lhs >= rhs {
                    self.branch_to_target(hart_id)?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bltu => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;  // rs1
                let rhs = self.get_x(hart_id, inst.get_r2())?;  // rs2

                if lhs < rhs {
                    self.branch_to_target(hart_id)?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Bgeu => {
                let lhs = self.get_x(hart_id, inst.get_r1())?;  // rs1
                let rhs = self.get_x(hart_id, inst.get_r2())?;  // rs2

                if lhs >= rhs {
                    self.branch_to_target(hart_id)?;
                } else {
                    self.next_pc(hart_id)?;
                }
            }
            OpCode::Auipc => {
                let imm = self.get_resolved_i64(hart_id, inst)? << 12;
                let pc = self.get_pc(hart_id)? as u64;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    pc.wrapping_add_signed(imm),
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Jal => {
                let return_pc = self.get_pc(hart_id)? + PC_INCREMENT;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    return_pc as u64,
                )?;

                self.branch_to_target(hart_id)?;
            }
            OpCode::Jalr => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;
                // P0-5: Clear the lowest 2 bits (& !3) for 4-byte alignment.
                // The simulator only supports standard 4-byte RISC-V instructions;
                // 2-byte-aligned-but-not-4-byte-aligned targets would miss the
                // inst_cache lookup and silently mark the hart as Finished.
                let target = base.wrapping_add_signed(imm) as usize & !3usize;
                let return_pc = self.get_pc(hart_id)? + PC_INCREMENT;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    return_pc as u64,
                )?;

                self.set_pc(hart_id, Some(target))?;
            }
            OpCode::Faddd=> {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs + rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fsubd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs - rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fmuld => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs * rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fdivd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_f(
                    hart_id,
                    inst.get_r0(),
                    lhs / rhs,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Fadds => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs + rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fsubs => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs - rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fmuls => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs * rhs,
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fdivs => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_f32(
                    hart_id,
                    inst.get_r0(),
                    lhs / rhs,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Fsqrts => {
                let val = self.get_f32(hart_id, inst.get_r1())?;
                self.set_f32(hart_id, inst.get_r0(), val.sqrt());
                self.next_pc(hart_id)?;
            }
            OpCode::Fsqrtd => {
                let val = self.get_f(hart_id, inst.get_r1())?;
                self.set_f(hart_id, inst.get_r0(), val.sqrt());
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjs => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;
                let result = f32::from_bits((lhs.to_bits() & !(1u32 << 31)) | (rhs.to_bits() & (1u32 << 31)));
                self.set_f32(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjns => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;
                let result = f32::from_bits((lhs.to_bits() & !(1u32 << 31)) | (!rhs.to_bits() & (1u32 << 31)));
                self.set_f32(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjxs => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;
                let result = f32::from_bits(lhs.to_bits() ^ (rhs.to_bits() & (1u32 << 31)));
                self.set_f32(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;
                let result = f64::from_bits((lhs.to_bits() & !(1u64 << 63)) | (rhs.to_bits() & (1u64 << 63)));
                self.set_f(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjnd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;
                let result = f64::from_bits((lhs.to_bits() & !(1u64 << 63)) | (!rhs.to_bits() & (1u64 << 63)));
                self.set_f(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fsgnjxd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;
                let result = f64::from_bits(lhs.to_bits() ^ (rhs.to_bits() & (1u64 << 63)));
                self.set_f(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fmins => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;
                self.set_f32(hart_id, inst.get_r0(), lhs.min(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Fmaxs => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;
                self.set_f32(hart_id, inst.get_r0(), lhs.max(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Fmind => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;
                self.set_f(hart_id, inst.get_r0(), lhs.min(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Fmaxd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;
                self.set_f(hart_id, inst.get_r0(), lhs.max(rhs));
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtsd => {
                let val = self.get_f32(hart_id, inst.get_r1())? as f64;
                self.set_f(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtds => {
                let val = self.get_f(hart_id, inst.get_r1())? as f32;
                self.set_f32(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            // NOTE: Floating-point to integer conversions (Fcvtws, Fcvtwd, Fcvtls, Fcvtld, etc.)
            // use Rust's `as` operator which truncates toward zero (RTZ). The RISC-V spec
            // requires honoring the FCSR.frm field (default: RNE, round-to-nearest-even).
            // This simulator currently only supports RTZ mode. Programs relying on other
            // rounding modes (RNE, RUP, RDN, RMM) may produce incorrect results.
            OpCode::Fcvtws => {
                let val = self.get_f32(hart_id, inst.get_r1())? as i32 as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtwus => {
                let val = self.get_f32(hart_id, inst.get_r1())?;
                let result = (val as u32) as u64;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtwd => {
                let val = self.get_f(hart_id, inst.get_r1())? as i32 as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtwud => {
                let val = self.get_f(hart_id, inst.get_r1())?;
                let result = (val as u32) as u64;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtsw => {
                let val = self.get_x(hart_id, inst.get_r1())? as i32 as f32;
                self.set_f32(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtswu => {
                let val = self.get_x(hart_id, inst.get_r1())? as u32 as f32;
                self.set_f32(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtdw => {
                let val = self.get_x(hart_id, inst.get_r1())? as i32 as f64;
                self.set_f(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtdwu => {
                let val = self.get_x(hart_id, inst.get_r1())? as u32 as f64;
                self.set_f(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtls => {
                let val = self.get_f32(hart_id, inst.get_r1())? as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtlus => {
                let val = self.get_f32(hart_id, inst.get_r1())? as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtld => {
                let val = self.get_f(hart_id, inst.get_r1())? as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtlud => {
                let val = self.get_f(hart_id, inst.get_r1())? as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtsl => {
                let val = self.get_x(hart_id, inst.get_r1())? as i64 as f32;
                self.set_f32(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtslu => {
                let val = self.get_x(hart_id, inst.get_r1())? as f32;
                self.set_f32(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtdl => {
                let val = self.get_x(hart_id, inst.get_r1())? as i64 as f64;
                self.set_f(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fcvtdlu => {
                let val = self.get_x(hart_id, inst.get_r1())? as f64;
                self.set_f(hart_id, inst.get_r0(), val);
                self.next_pc(hart_id)?;
            }
            OpCode::Fclasss => {
                let val = self.get_f32(hart_id, inst.get_r1())?;
                let bits = val.to_bits();
                let result: u64 = if val.is_infinite() {
                    if bits & (1u32 << 31) != 0 { 1 << 0 } else { 1 << 7 }
                } else if val.is_nan() {
                    if bits & (1 << 22) != 0 { 1 << 8 } else { 1 << 9 }
                } else if val == 0.0 {
                    if bits & (1u32 << 31) != 0 { 1 << 3 } else { 1 << 4 }
                } else if val.is_subnormal() {
                    if bits & (1u32 << 31) != 0 { 1 << 2 } else { 1 << 5 }
                } else {
                    if bits & (1u32 << 31) != 0 { 1 << 1 } else { 1 << 6 }
                };
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fclassd => {
                let val = self.get_f(hart_id, inst.get_r1())?;
                let bits = val.to_bits();
                let result: u64 = if val.is_infinite() {
                    if bits & (1u64 << 63) != 0 { 1 << 0 } else { 1 << 7 }
                } else if val.is_nan() {
                    if bits & (1u64 << 51) != 0 { 1 << 8 } else { 1 << 9 }
                } else if val == 0.0 {
                    if bits & (1u64 << 63) != 0 { 1 << 3 } else { 1 << 4 }
                } else if val.is_subnormal() {
                    if bits & (1u64 << 63) != 0 { 1 << 2 } else { 1 << 5 }
                } else {
                    if bits & (1u64 << 63) != 0 { 1 << 1 } else { 1 << 6 }
                };
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Fmvxw => {
                let val = self.get_f32(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), val.to_bits() as u64);
                self.next_pc(hart_id)?;
            }
            OpCode::Fmvwx => {
                let val = self.get_x(hart_id, inst.get_r1())? as u32;
                self.set_f32(hart_id, inst.get_r0(), f32::from_bits(val));
                self.next_pc(hart_id)?;
            }
            OpCode::Feqd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs == rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fltd => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fled => {
                let lhs = self.get_f(hart_id, inst.get_r1())?;
                let rhs = self.get_f(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs <= rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Feqs => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs == rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Flts => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs < rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }

            OpCode::Fles => {
                let lhs = self.get_f32(hart_id, inst.get_r1())?;
                let rhs = self.get_f32(hart_id, inst.get_r2())?;

                self.set_x(
                    hart_id,
                    inst.get_r0(),
                    if lhs <= rhs { 1 } else { 0 },
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Fmvdx => {
                let value = self.get_x(hart_id, inst.get_r1())?;
                self.set_f_bits(hart_id, inst.get_r0(), value)?;
                self.next_pc(hart_id)?;
            }
            OpCode::Fmvxd => {
                let value = self.get_f_bits(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), value)?;
                self.next_pc(hart_id)?;
            }
            OpCode::Fld => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);
                let bits = self.processors[proc_idx].memory.read_u64(addr)?;

                self.set_f_bits(
                    hart_id,
                    inst.get_r0(),
                    bits,
                )?;

                self.next_pc(hart_id)?;
            }
            OpCode::Fsd => {
                let bits = self.get_f_bits(hart_id, inst.get_r2())?;  // rs2 = float reg to store
                let base = self.get_x(hart_id, inst.get_r1())?;       // rs1 = base address
                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);

                self.processors[proc_idx].memory.write_u64(addr,bits)?;

                self.next_pc(hart_id)?;
            }
            OpCode::Fmadds => {
                let a = self.get_f32(hart_id, inst.get_r1())?;
                let b = self.get_f32(hart_id, inst.get_r2())?;
                let c = self.get_f32(hart_id, inst.get_r3())?;
                self.set_f32(hart_id, inst.get_r0(), a.mul_add(b, c));
                self.next_pc(hart_id)?;
            }
            OpCode::Fmsubs => {
                let a = self.get_f32(hart_id, inst.get_r1())?;
                let b = self.get_f32(hart_id, inst.get_r2())?;
                let c = self.get_f32(hart_id, inst.get_r3())?;
                self.set_f32(hart_id, inst.get_r0(), a.mul_add(b, -c));
                self.next_pc(hart_id)?;
            }
            OpCode::Fnmsubs => {
                let a = self.get_f32(hart_id, inst.get_r1())?;
                let b = self.get_f32(hart_id, inst.get_r2())?;
                let c = self.get_f32(hart_id, inst.get_r3())?;
                self.set_f32(hart_id, inst.get_r0(), -(a.mul_add(b, -c)));
                self.next_pc(hart_id)?;
            }
            OpCode::Fnmadds => {
                let a = self.get_f32(hart_id, inst.get_r1())?;
                let b = self.get_f32(hart_id, inst.get_r2())?;
                let c = self.get_f32(hart_id, inst.get_r3())?;
                self.set_f32(hart_id, inst.get_r0(), -(a.mul_add(b, c)));
                self.next_pc(hart_id)?;
            }
            OpCode::Fmaddd => {
                let a = self.get_f(hart_id, inst.get_r1())?;
                let b = self.get_f(hart_id, inst.get_r2())?;
                let c = self.get_f(hart_id, inst.get_r3())?;
                self.set_f(hart_id, inst.get_r0(), a.mul_add(b, c));
                self.next_pc(hart_id)?;
            }
            OpCode::Fmsubd => {
                let a = self.get_f(hart_id, inst.get_r1())?;
                let b = self.get_f(hart_id, inst.get_r2())?;
                let c = self.get_f(hart_id, inst.get_r3())?;
                self.set_f(hart_id, inst.get_r0(), a.mul_add(b, -c));
                self.next_pc(hart_id)?;
            }
            OpCode::Fnmsubd => {
                let a = self.get_f(hart_id, inst.get_r1())?;
                let b = self.get_f(hart_id, inst.get_r2())?;
                let c = self.get_f(hart_id, inst.get_r3())?;
                self.set_f(hart_id, inst.get_r0(), -(a.mul_add(b, -c)));
                self.next_pc(hart_id)?;
            }
            OpCode::Fnmaddd => {
                let a = self.get_f(hart_id, inst.get_r1())?;
                let b = self.get_f(hart_id, inst.get_r2())?;
                let c = self.get_f(hart_id, inst.get_r3())?;
                self.set_f(hart_id, inst.get_r0(), -(a.mul_add(b, c)));
                self.next_pc(hart_id)?;
            }
            OpCode::Flw => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);
                let bits = self.memory.read_u32(addr)?;
                self.set_f32(hart_id, inst.get_r0(), f32::from_bits(bits));
                self.next_pc(hart_id)?;
            }
            OpCode::Fsw => {
                let val = self.get_f32(hart_id, inst.get_r0())?;
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_resolved_i64(hart_id, inst)?;
                let addr = base.wrapping_add_signed(imm);
                self.memory.write_u32(addr, val.to_bits());
                self.next_pc(hart_id)?;
            }
            // ============================================================
            // Krypto: Zbkb - Pack/Packh/Packw/Brev8/Zip/Unzip
            // ============================================================
            OpCode::Pack => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let rs2 = self.get_x(hart_id, inst.get_r2())?;
                let result = (rs1 & 0xFFFFFFFF) | (rs2 << 32);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Packh => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let rs2 = self.get_x(hart_id, inst.get_r2())?;
                let mut result: u64 = 0;
                for i in 0..4 {
                    let rs1_byte = (rs1 >> (i * 8)) & 0xFF;
                    let rs2_byte = (rs2 >> (i * 8)) & 0xFF;
                    result |= rs1_byte << (i * 16);
                    result |= rs2_byte << (i * 16 + 8);
                }
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Packw => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let rs2 = self.get_x(hart_id, inst.get_r2())?;
                let lo = rs1 & 0xFFFF;
                let hi = rs2 & 0xFFFF;
                let result = (lo | (hi << 16)) as i32 as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Brev8 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let mut result: u64 = 0;
                for i in 0..8 {
                    let byte = ((val >> (i * 8)) & 0xFF) as u8;
                    let rev = byte.reverse_bits();
                    result |= (rev as u64) << (i * 8);
                }
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Zip => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let mut result: u64 = 0;
                for i in 0..32 {
                    let low_bit = (val >> i) & 1;
                    let high_bit = (val >> (i + 32)) & 1;
                    result |= low_bit << (2 * i);
                    result |= high_bit << (2 * i + 1);
                }
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Unzip => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let mut result: u64 = 0;
                for i in 0..32 {
                    let low_bit = (val >> (2 * i)) & 1;
                    let high_bit = (val >> (2 * i + 1)) & 1;
                    result |= low_bit << i;
                    result |= high_bit << (i + 32);
                }
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // ============================================================
            // Krypto: Zbkx - Xperm4/Xperm8
            // ============================================================
            OpCode::Xperm4 => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let rs2 = self.get_x(hart_id, inst.get_r2())?;
                let mut result: u64 = 0;
                for i in 0..16 {
                    let idx = ((rs1 >> (i * 4)) & 0xF) as usize;
                    let nibble = (rs2 >> (idx * 4)) & 0xF;
                    result |= nibble << (i * 4);
                }
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Xperm8 => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let rs2 = self.get_x(hart_id, inst.get_r2())?;
                let mut result: u64 = 0;
                for i in 0..8 {
                    let idx = ((rs1 >> (i * 8)) & 0xFF) as usize;
                    // Per spec: if idx >= 8, result byte is 0
                    if idx < 8 {
                        let byte = (rs2 >> (idx * 8)) & 0xFF;
                        result |= byte << (i * 8);
                    }
                }
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // ============================================================
            // Krypto: Zknd/Zkne - AES instructions
            // ============================================================

            // AES decrypt (final round): InvMixColumns(InvSubBytes(rs1)) ^ rs2
            OpCode::Aes64ds => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let rs2 = self.get_x(hart_id, inst.get_r2())?;
                let result = aes64_inv_mix_columns(aes64_inv_sub_bytes(rs1)) ^ rs2;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // AES decrypt middle round: InvMixColumns(rs1) ^ rs2
            OpCode::Aes64dsm => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let rs2 = self.get_x(hart_id, inst.get_r2())?;
                let result = aes64_inv_mix_columns(rs1) ^ rs2;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // AES encrypt (final round): MixColumns(SubBytes(rs1)) ^ rs2
            OpCode::Aes64es => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let rs2 = self.get_x(hart_id, inst.get_r2())?;
                let result = aes64_mix_columns(aes64_sub_bytes(rs1)) ^ rs2;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // AES encrypt middle round: MixColumns(rs1) ^ rs2
            OpCode::Aes64esm => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let rs2 = self.get_x(hart_id, inst.get_r2())?;
                let result = aes64_mix_columns(rs1) ^ rs2;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // AES intermediate mix columns: InvMixColumns(rs1)
            OpCode::Aes64im => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let result = aes64_inv_mix_columns(rs1);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // AES key schedule round constant: extract rnum from encoded imm (0x310 | rnum)
            OpCode::Aes64ks1i => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let rnum = (self.get_resolved_u64(hart_id, inst)? & 0xF) as u32;
                let result = aes64_ks1i(rs1, rnum);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // AES key schedule word XOR
            OpCode::Aes64ks2 => {
                let rs1 = self.get_x(hart_id, inst.get_r1())?;
                let rs2 = self.get_x(hart_id, inst.get_r2())?;
                let result = aes64_ks2(rs1, rs2);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // ============================================================
            // Krypto: Zknh - SHA256 instructions
            // ============================================================
            OpCode::Sha256sig0 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let result = val.rotate_right(7) ^ val.rotate_right(18) ^ (val >> 3);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Sha256sig1 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let result = val.rotate_right(17) ^ val.rotate_right(19) ^ (val >> 10);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Sha256sum0 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let result = val.rotate_right(2) ^ val.rotate_right(13) ^ val.rotate_right(22);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Sha256sum1 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let result = val.rotate_right(6) ^ val.rotate_right(11) ^ val.rotate_right(25);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // ============================================================
            // Krypto: Zknh - SHA512 instructions
            // ============================================================
            OpCode::Sha512sig0 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let result = val.rotate_right(1) ^ val.rotate_right(8) ^ (val >> 7);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Sha512sig1 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let result = val.rotate_right(19) ^ val.rotate_right(61) ^ (val >> 6);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Sha512sum0 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let result = val.rotate_right(28) ^ val.rotate_right(34) ^ val.rotate_right(39);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Sha512sum1 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let result = val.rotate_right(14) ^ val.rotate_right(18) ^ val.rotate_right(41);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // ============================================================
            // Krypto: Zksh - SM3 instructions
            // ============================================================
            OpCode::Sm3p0 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let result = val ^ val.rotate_right(9) ^ val.rotate_right(17);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            OpCode::Sm3p1 => {
                let val = self.get_x(hart_id, inst.get_r1())?;
                let result = val ^ val.rotate_right(15) ^ val.rotate_right(23);
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc(hart_id)?;
            }
            // ============================================================
            // C Extension - Compressed instructions (2-byte)
            // ============================================================

            // C.LW: Load word from memory (rs1 + offset)
            OpCode::Clw => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = base.wrapping_add(imm);
                let val = self.memory.read_u32(addr)? as i32 as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.LWSP: Load word from stack pointer (x2 + offset)
            // PEG: rd, imm → r0=rd, r1=None. Must use sp (x2) as base.
            OpCode::Clwsp => {
                let sp = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = sp.wrapping_add(imm);
                let val = self.memory.read_u32(addr)? as i32 as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.LD: Load doubleword from memory
            OpCode::Cld => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = base.wrapping_add(imm);
                let val = self.memory.read_u64(addr)?;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.LDSP: Load doubleword from stack pointer
            // PEG: rd, imm → r0=rd, r1=None. Must use sp (x2) as base.
            OpCode::Cldsp => {
                let sp = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = sp.wrapping_add(imm);
                let val = self.memory.read_u64(addr)?;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.LQ: Load quadword (128-bit) - simplified to load lower 64 bits
            OpCode::Clq => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = base.wrapping_add(imm);
                let val = self.memory.read_u64(addr)?;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.LQSP: Load quadword from stack pointer - simplified
            // PEG: rd, imm → r0=rd, r1=None. Must use sp (x2) as base.
            OpCode::Clqsp => {
                let sp = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = sp.wrapping_add(imm);
                let val = self.memory.read_u64(addr)?;
                self.set_x(hart_id, inst.get_r0(), val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SW: Store word to memory
            // PEG: c.sw rs1, rs2, imm → r0=rs1(base), r1=rs2(value)
            OpCode::Csw => {
                let base = self.get_x(hart_id, inst.get_r0())?;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = base.wrapping_add(imm);
                let val = self.get_x(hart_id, inst.get_r1())? as u32;
                self.memory.write_u32(addr, val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SWSP: Store word to stack pointer
            // PEG: rs2, imm → r0=rs2, r1=None. Must use sp (x2) as base.
            OpCode::Cswsp => {
                let sp = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = sp.wrapping_add(imm);
                let val = self.get_x(hart_id, inst.get_r0())? as u32;
                self.memory.write_u32(addr, val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SD: Store doubleword to memory
            // PEG: c.sd rs1, rs2, imm → r0=rs1(base), r1=rs2(value)
            OpCode::Csd => {
                let base = self.get_x(hart_id, inst.get_r0())?;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = base.wrapping_add(imm);
                let val = self.get_x(hart_id, inst.get_r1())?;
                self.memory.write_u64(addr, val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SDSP: Store doubleword to stack pointer
            // PEG: rs2, imm → r0=rs2, r1=None. Must use sp (x2) as base.
            OpCode::Csdsp => {
                let sp = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = sp.wrapping_add(imm);
                let val = self.get_x(hart_id, inst.get_r0())?;
                self.memory.write_u64(addr, val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SQ: Store quadword - simplified to store 64 bits
            // PEG: c.sq rs1, rs2, imm → r0=rs1(base), r1=rs2(value)
            OpCode::Csq => {
                let base = self.get_x(hart_id, inst.get_r0())?;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = base.wrapping_add(imm);
                let val = self.get_x(hart_id, inst.get_r1())?;
                self.memory.write_u64(addr, val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SQSP: Store quadword to stack pointer - simplified
            // PEG: rs2, imm → r0=rs2, r1=None. Must use sp (x2) as base.
            OpCode::Csqsp => {
                let sp = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = sp.wrapping_add(imm);
                let val = self.get_x(hart_id, inst.get_r0())?;
                self.memory.write_u64(addr, val);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.FLW: Load float (single-precision)
            OpCode::Cflw => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = base.wrapping_add(imm);
                let val = self.memory.read_u32(addr)?;
                self.set_f32(hart_id, inst.get_r0(), f32::from_bits(val));
                self.next_pc_by(hart_id, 2)?;
            }
            // C.FLWSP: Load float from stack pointer
            // PEG: rd, imm → r0=rd, r1=None. Must use sp (x2) as base.
            OpCode::Cflwsp => {
                let sp = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = sp.wrapping_add(imm);
                let val = self.memory.read_u32(addr)?;
                self.set_f32(hart_id, inst.get_r0(), f32::from_bits(val));
                self.next_pc_by(hart_id, 2)?;
            }
            // C.FLD: Load double
            OpCode::Cfld => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = base.wrapping_add(imm);
                let val = self.memory.read_u64(addr)?;
                self.set_f(hart_id, inst.get_r0(), f64::from_bits(val));
                self.next_pc_by(hart_id, 2)?;
            }
            // C.FLDSP: Load double from stack pointer
            // PEG: rd, imm → r0=rd, r1=None. Must use sp (x2) as base.
            OpCode::Cfldsp => {
                let sp = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = sp.wrapping_add(imm);
                let val = self.memory.read_u64(addr)?;
                self.set_f(hart_id, inst.get_r0(), f64::from_bits(val));
                self.next_pc_by(hart_id, 2)?;
            }
            // C.FSW: Store float
            OpCode::Cfsw => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = base.wrapping_add(imm);
                let val = self.get_f32(hart_id, inst.get_r0())?;
                self.memory.write_u32(addr, val.to_bits());
                self.next_pc_by(hart_id, 2)?;
            }
            // C.FSWSP: Store float to stack pointer
            // PEG: rs2, imm → r0=rs2, r1=None. Must use sp (x2) as base.
            OpCode::Cfswsp => {
                let sp = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = sp.wrapping_add(imm);
                let val = self.get_f32(hart_id, inst.get_r0())?;
                self.memory.write_u32(addr, val.to_bits());
                self.next_pc_by(hart_id, 2)?;
            }
            // C.FSD: Store double
            OpCode::Cfsd => {
                let base = self.get_x(hart_id, inst.get_r1())?;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = base.wrapping_add(imm);
                let val = self.get_f(hart_id, inst.get_r0())?;
                self.memory.write_u64(addr, val.to_bits());
                self.next_pc_by(hart_id, 2)?;
            }
            // C.FSDSP: Store double to stack pointer
            // PEG: rs2, imm → r0=rs2, r1=None. Must use sp (x2) as base.
            OpCode::Cfsdsp => {
                let sp = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                let addr = sp.wrapping_add(imm);
                let val = self.get_f(hart_id, inst.get_r0())?;
                self.memory.write_u64(addr, val.to_bits());
                self.next_pc_by(hart_id, 2)?;
            }
            // C.ADDI4SPN: Add immediate * 4 to sp, store in rd
            // PEG: rd, imm → r0=rd, r1=None. Must use sp (x2).
            OpCode::Caddi4spn => {
                let sp_val = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_u64_from_imm(hart_id, inst.get_imm())?;
                self.set_x(hart_id, inst.get_r0(), sp_val.wrapping_add(imm));
                self.next_pc_by(hart_id, 2)?;
            }
            // C.ADDI: Add immediate (rd = rd + imm). PEG: rd, imm → r0=rd, r1=None→x0.
            // Note: c.addi x0, imm is c.nop (handled separately).
            OpCode::Caddi => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm())?;
                self.set_x(hart_id, inst.get_r0(), (rd_val as i64).wrapping_add(imm) as u64);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.ADDIW: Add word immediate (rd = (rd as i32) + imm). PEG: rd, imm → r0=rd, r1=None→x0.
            OpCode::Caddiw => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm())?;
                let result = (rd_val as i32).wrapping_add(imm as i32) as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.ADDI16SP: Add immediate * 16 to sp. PEG: c.addi16sp x0, imm → r0=x0, r1=None.
            // Must use sp (x2) as both source and destination.
            OpCode::Caddi16sp => {
                let sp_val = self.get_hart(hart_id).unwrap().x.regs[2].value;
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm())?;
                let result = (sp_val as i64).wrapping_add(imm) as u64;
                let hart = self.get_hart_mut(hart_id).unwrap();
                hart.x.regs[2].value = result;
                self.next_pc_by(hart_id, 2)?;
            }
            // C.LI: Load immediate (rd = imm, rs1 = x0)
            OpCode::Cli => {
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm())?;
                self.set_x(hart_id, inst.get_r0(), imm as u64);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.LUI: Load upper immediate (imm is already shifted by PEG: create_compact_inc_from_current with bits 17..12)
            OpCode::Clui => {
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm())?;
                // PEG stores the raw imm value; lui shifts left by 12
                self.set_x(hart_id, inst.get_r0(), ((imm as i64) << 12) as u64);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SLLI: Shift left logical immediate (RV32). PEG: rd, shamt → r0=rd, r1=None→x0.
            // Semantics: rd = rd << shamt, so use r0 as source.
            OpCode::Cslli => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let shamt = self.get_u64_from_imm(hart_id, inst.get_imm())? & 0x1F;
                self.set_x(hart_id, inst.get_r0(), rd_val << shamt);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SLLI64: Shift left logical immediate (RV64)
            OpCode::Cslli64 => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let shamt = self.get_u64_from_imm(hart_id, inst.get_imm())? & 0x3F;
                self.set_x(hart_id, inst.get_r0(), rd_val << shamt);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SRLI: Shift right logical immediate. PEG: rd, shamt → r0=rd.
            OpCode::Csrli => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let shamt = self.get_u64_from_imm(hart_id, inst.get_imm())? & 0x1F;
                self.set_x(hart_id, inst.get_r0(), rd_val >> shamt);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SRLI64: Shift right logical immediate (RV64)
            OpCode::Csrli64 => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let shamt = self.get_u64_from_imm(hart_id, inst.get_imm())? & 0x3F;
                self.set_x(hart_id, inst.get_r0(), rd_val >> shamt);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SRAI: Shift right arithmetic immediate. PEG: rd, shamt → r0=rd.
            OpCode::Csrai => {
                let rd_val = self.get_x(hart_id, inst.get_r0())? as i32 as i64;
                let shamt = self.get_u64_from_imm(hart_id, inst.get_imm())? & 0x1F;
                self.set_x(hart_id, inst.get_r0(), (rd_val >> shamt) as u64);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SRAI64: Shift right arithmetic immediate (RV64)
            OpCode::Csrai64 => {
                let rd_val = self.get_x(hart_id, inst.get_r0())? as i64;
                let shamt = self.get_u64_from_imm(hart_id, inst.get_imm())? & 0x3F;
                self.set_x(hart_id, inst.get_r0(), (rd_val >> shamt) as u64);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.ANDI: And immediate (rd = rd & imm). PEG: rd, imm → r0=rd, r1=None→x0.
            OpCode::Candi => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let imm = self.get_i64_from_imm(hart_id, inst.get_imm())?;
                self.set_x(hart_id, inst.get_r0(), rd_val & (imm as u64));
                self.next_pc_by(hart_id, 2)?;
            }
            // C.MV: Move (rd = rs2). PEG: rd, rs1 → r0=rd, r1=rs2
            OpCode::Cmv => {
                let rs2 = self.get_x(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), rs2);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.ADD: Add (rd = rd + rs2). PEG: rd, rs1 → r0=rd, r1=rs2
            OpCode::Cadd => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let rs2 = self.get_x(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), rd_val.wrapping_add(rs2));
                self.next_pc_by(hart_id, 2)?;
            }
            // C.ADDW: Add word (rd = rd + rs2). PEG: rd, rs1 → r0=rd, r1=rs2
            OpCode::Caddw => {
                let rd_val = self.get_x(hart_id, inst.get_r0())? as i32;
                let rs2 = self.get_x(hart_id, inst.get_r1())? as i32;
                let result = rd_val.wrapping_add(rs2) as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SUB: Subtract (rd = rd - rs2). PEG: rd, rs1 → r0=rd, r1=rs2
            OpCode::Csub => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let rs2 = self.get_x(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), rd_val.wrapping_sub(rs2));
                self.next_pc_by(hart_id, 2)?;
            }
            // C.SUBW: Subtract word (rd = rd - rs2). PEG: rd, rs1 → r0=rd, r1=rs2
            OpCode::Csubw => {
                let rd_val = self.get_x(hart_id, inst.get_r0())? as i32;
                let rs2 = self.get_x(hart_id, inst.get_r1())? as i32;
                let result = rd_val.wrapping_sub(rs2) as i64 as u64;
                self.set_x(hart_id, inst.get_r0(), result);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.XOR: XOR (rd = rd ^ rs2). PEG: rd, rs1 → r0=rd, r1=rs2
            OpCode::Cxor => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let rs2 = self.get_x(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), rd_val ^ rs2);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.OR: OR (rd = rd | rs2). PEG: rd, rs1 → r0=rd, r1=rs2
            OpCode::Cor => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let rs2 = self.get_x(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), rd_val | rs2);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.AND: AND (rd = rd & rs2). PEG: rd, rs1 → r0=rd, r1=rs2
            OpCode::Cand => {
                let rd_val = self.get_x(hart_id, inst.get_r0())?;
                let rs2 = self.get_x(hart_id, inst.get_r1())?;
                self.set_x(hart_id, inst.get_r0(), rd_val & rs2);
                self.next_pc_by(hart_id, 2)?;
            }
            // C.BEQZ: Branch if equal to zero. PEG: rs1, imm → r0=rs1, r1=None.
            OpCode::Cbeqz => {
                let rs1 = self.get_x(hart_id, inst.get_r0())?;
                if rs1 == 0 {
                    let offset = self.get_i64_from_imm(hart_id, inst.get_imm())?;
                    let hart = self.get_hart_mut(hart_id).unwrap();
                    hart.pc = ((hart.pc as i64).wrapping_add(offset)) as usize;
                } else {
                    self.next_pc_by(hart_id, 2)?;
                }
            }
            // C.BNEZ: Branch if not equal to zero. PEG: rs1, imm → r0=rs1, r1=None.
            OpCode::Cbnez => {
                let rs1 = self.get_x(hart_id, inst.get_r0())?;
                if rs1 != 0 {
                    let offset = self.get_i64_from_imm(hart_id, inst.get_imm())?;
                    let hart = self.get_hart_mut(hart_id).unwrap();
                    hart.pc = ((hart.pc as i64).wrapping_add(offset)) as usize;
                } else {
                    self.next_pc_by(hart_id, 2)?;
                }
            }
            // C.J: Jump (unconditional)
            OpCode::Cj => {
                let offset = self.get_i64_from_imm(hart_id, inst.get_imm())?;
                let hart = self.get_hart_mut(hart_id).unwrap();
                hart.pc = ((hart.pc as i64).wrapping_add(offset)) as usize;
            }
            // C.JAL: Jump and link (RV32 only, save return address in ra)
            OpCode::Cjal => {
                let ra_idx = self.registers.get_register_value(Some(&"ra".to_string())).unwrap() as usize;
                let offset = self.get_i64_from_imm(hart_id, inst.get_imm())?;
                let hart = self.get_hart_mut(hart_id).unwrap();
                hart.x.regs[ra_idx].value = (hart.pc + 2) as u64;
                hart.pc = ((hart.pc as i64).wrapping_add(offset)) as usize;
            }
            // C.JR: Jump register (rs1 = target, rd = x0 for C.JR)
            OpCode::Cjr => {
                let target = self.get_x(hart_id, inst.get_r1())?;
                let hart = self.get_hart_mut(hart_id).unwrap();
                hart.pc = target as usize;
            }
            // C.JALR: Jump and link register
            OpCode::Cjalr => {
                let target = self.get_x(hart_id, inst.get_r1())?;
                let ra_idx = self.registers.get_register_value(Some(&"ra".to_string())).unwrap() as usize;
                let hart = self.get_hart_mut(hart_id).unwrap();
                hart.x.regs[ra_idx].value = (hart.pc + 2) as u64;
                hart.pc = target as usize;
            }
            // C.EBREAK: Breakpoint (same as EBREAK but compact)
            OpCode::Cebreak => {
                let hart = self.get_hart_mut(hart_id).unwrap();
                hart.take_trap(EXCEPTION_BREAKPOINT, hart.pc as u64, false);
            }
            // C.NOP: No operation (C.ADDI x0, x0, 0)
            OpCode::Cnop => {
                self.next_pc_by(hart_id, 2)?;
            }
            // ============================================================
            // CSR Instructions (Zicsr extension)
            // ============================================================
            // CSRRW rd, csr, rs1: atomic read/write CSR
            //   rd = CSR[csr]; CSR[csr] = rs1
            OpCode::Csrrw => {
                let csr_addr = self.get_resolved_u64(hart_id, inst)?;
                let rs1_val = self.get_x(hart_id, inst.get_r1())?;
                let old_val = self.read_csr(hart_id, csr_addr);
                self.write_csr(hart_id, csr_addr, rs1_val);
                self.set_x(hart_id, inst.get_r0(), old_val);
                self.next_pc(hart_id)?;
            }
            // CSRRS rd, csr, rs1: atomic read and set bits in CSR
            //   rd = CSR[csr]; CSR[csr] = CSR[csr] | rs1
            OpCode::Csrrs => {
                let csr_addr = self.get_resolved_u64(hart_id, inst)?;
                let rs1_val = self.get_x(hart_id, inst.get_r1())?;
                let old_val = self.read_csr(hart_id, csr_addr);
                let rs1_idx = self.xreg(&inst.get_r1().cloned())?;
                if rs1_idx != 0 { // if rs1 is x0, don't write (CSRRS pseudo-instruction)
                    self.write_csr(hart_id, csr_addr, old_val | rs1_val);
                }
                self.set_x(hart_id, inst.get_r0(), old_val);
                self.next_pc(hart_id)?;
            }
            // CSRRC rd, csr, rs1: atomic read and clear bits in CSR
            //   rd = CSR[csr]; CSR[csr] = CSR[csr] & ~rs1
            OpCode::Csrrc => {
                let csr_addr = self.get_resolved_u64(hart_id, inst)?;
                let rs1_val = self.get_x(hart_id, inst.get_r1())?;
                let old_val = self.read_csr(hart_id, csr_addr);
                let rs1_idx = self.xreg(&inst.get_r1().cloned())?;
                if rs1_idx != 0 { // if rs1 is x0, don't write (CSRRC pseudo-instruction)
                    self.write_csr(hart_id, csr_addr, old_val & !rs1_val);
                }
                self.set_x(hart_id, inst.get_r0(), old_val);
                self.next_pc(hart_id)?;
            }
            // CSRRWI rd, csr, zimm: atomic read/write CSR (immediate)
            //   rd = CSR[csr]; CSR[csr] = zimm (zero-extended)
            OpCode::Csrrwi => {
                let csr_addr = self.get_resolved_u64(hart_id, inst)?;
                let zimm = self.get_x(hart_id, inst.get_r1())?; // r1 is uimm[4:0] = zimm
                let old_val = self.read_csr(hart_id, csr_addr);
                self.write_csr(hart_id, csr_addr, zimm);
                self.set_x(hart_id, inst.get_r0(), old_val);
                self.next_pc(hart_id)?;
            }
            // CSRRSI rd, csr, zimm: atomic read and set bits in CSR (immediate)
            //   rd = CSR[csr]; CSR[csr] = CSR[csr] | zimm
            OpCode::Csrrsi => {
                let csr_addr = self.get_resolved_u64(hart_id, inst)?;
                let zimm = self.get_x(hart_id, inst.get_r1())?; // r1 is uimm[4:0] = zimm
                let old_val = self.read_csr(hart_id, csr_addr);
                if zimm != 0 { // if zimm is 0, don't write
                    self.write_csr(hart_id, csr_addr, old_val | zimm);
                }
                self.set_x(hart_id, inst.get_r0(), old_val);
                self.next_pc(hart_id)?;
            }
            // CSRRCI rd, csr, zimm: atomic read and clear bits in CSR (immediate)
            //   rd = CSR[csr]; CSR[csr] = CSR[csr] & ~zimm
            OpCode::Csrrci => {
                let csr_addr = self.get_resolved_u64(hart_id, inst)?;
                let zimm = self.get_x(hart_id, inst.get_r1())?; // r1 is uimm[4:0] = zimm
                let old_val = self.read_csr(hart_id, csr_addr);
                if zimm != 0 { // if zimm is 0, don't write
                    self.write_csr(hart_id, csr_addr, old_val & !zimm);
                }
                self.set_x(hart_id, inst.get_r0(), old_val);
                self.next_pc(hart_id)?;
            }
            // ECALL: Environment call (trap to higher privilege)
            OpCode::Ecall => {
                let hart = self.get_hart_mut(hart_id).unwrap();
                hart.take_trap(EXCEPTION_ENVIRONMENT_CALL_FROM_M, hart.pc as u64, false);
            }
            // EBREAK: Environment breakpoint
            OpCode::Ebreak => {
                let hart = self.get_hart_mut(hart_id).unwrap();
                hart.take_trap(EXCEPTION_BREAKPOINT, hart.pc as u64, false);
            }
            // MRET: Machine trap return
            OpCode::Mret => {
                let hart = self.get_hart_mut(hart_id).unwrap();
                let mepc = hart.read_csr(0x341); // mepc
                let mstatus = hart.read_csr(0x300);
                // Set privilege to MPP (bits 11:12 of mstatus)
                let mpp = (mstatus >> 11) & 0x3;
                hart.privilege = match mpp {
                    0 => PrivilegeLevel::User,
                    1 => PrivilegeLevel::Supervisor,
                    _ => PrivilegeLevel::Machine,
                };
                // Set MIE = MPIE (restore previous interrupt enable)
                let mpie = (mstatus >> 7) & 1;
                hart.write_csr(0x300, (mstatus & !(1 << 3)) | (mpie << 3));
                // Set MPIE = 1
                hart.write_csr(0x300, hart.read_csr(0x300) | (1 << 7));
                hart.pc = mepc as usize;
            }
            // WFI: Wait for interrupt (treated as NOP for now)
            OpCode::Wfi => {
                self.next_pc(hart_id)?;
            }
            // FENCE/FENCE.I: Memory ordering (treated as NOP for single-hart)
            OpCode::Fence | OpCode::Fencei => {
                self.next_pc(hart_id)?;
            }
            _ => {
                return Err(DebuggerError::GeneralError(format!("unsupported instruction: {:?}", opcode)));
            }
        }

        Ok(())
    }

    /// get u64 value from Imm, if Imm is None, return 0
    fn get_u64_from_imm(&self, hart_id: HartId, imm: Option<&Imm>) -> Result<u64, DebuggerError> {
        match imm {
            Some(Imm::Value(s)) => core_utils::number::get_u64_from_str(s)
                .map_err(|e| DebuggerError::GeneralError(format!("failed to parse immediate '{}' as u64: {:?}", s, e))),
            Some(Imm::ImmMacro(n)) => {
                match n {
                    riscv_asm_lib::r5asm::imm_macro::ImmMacro::PtrSize => 
                        self.get_processor_from_hart_id(hart_id)
                            .map(|p| p.addressing.to_ptr_size())
                            .ok_or_else(|| DebuggerError::GeneralError("failed to resolve PtrSize for hart".to_string())),
                }
            }
            None => Ok(0),
        }
    }

    /// get i64 value from Imm, if Imm is None, return 0
    fn get_i64_from_imm(&self, hart_id: HartId, imm: Option<&Imm>) -> Result<i64, DebuggerError> {
        match imm {
            Some(Imm::Value(s)) => core_utils::number::get_i64_from_str(s)
                .map_err(|e| DebuggerError::GeneralError(format!("failed to parse immediate '{}' as i64: {:?}", s, e))),
            Some(Imm::ImmMacro(n)) => {
                match n {
                    riscv_asm_lib::r5asm::imm_macro::ImmMacro::PtrSize => 
                        self.get_processor_from_hart_id(hart_id)
                            .map(|p| p.addressing.to_ptr_size() as i64)
                            .ok_or_else(|| DebuggerError::GeneralError("failed to resolve PtrSize for hart".to_string())),
                }
            }
            None => Ok(0),
        }
    }

    fn next_pc(&mut self, hart_id: HartId) -> Result<(), DebuggerError> {
        self.next_pc_by(hart_id, PC_INCREMENT)
    }

    fn next_pc_by(&mut self, hart_id: HartId, delta: usize) -> Result<(), DebuggerError> {
        let err_msg = format!("invalid hart: {}", hart_id);
        let hart = self.get_hart_mut(hart_id).ok_or_else(|| DebuggerError::GeneralError(err_msg))?;
        hart.pc += delta;
        Ok(())
    }

    fn set_pc(&mut self, hart_id: HartId, pc: Option<usize>) -> Result<(), DebuggerError> {
        let err_msg = format!("invalid hart: {}", hart_id);
        let hart = self.get_hart_mut(hart_id).ok_or_else(|| DebuggerError::GeneralError(err_msg))?;
        hart.pc = pc.unwrap_or(hart.pc);
        Ok(())
    }

    fn get_pc(&self, hart_id: HartId) -> Result<usize, DebuggerError> {
        let err_msg = format!("invalid hart: {}", hart_id);
        let hart = self.get_hart(hart_id).ok_or_else(|| DebuggerError::GeneralError(err_msg))?;
        Ok(hart.pc)
    }

    pub fn lookup_register(&self, name: &str) -> Option<RegisterRef> {
        let index = self.registers.get_register_value(Some(&name.to_string()));
        match index {
            Ok(idx) => {
                let mut r = RegisterRef {
                    reg_type: RegisterType::Integer,
                    index: idx as usize,
                };

                let reg_type = if Register::is_float_register_name(name) {
                    RegisterType::Float
                }
                else if Register::is_vector_register_name(name) {
                    RegisterType::Vector
                }
                else if Register::is_csr_register_name(name) {
                    RegisterType::Csr
                }
                else {
                    RegisterType::Integer
                };

                r.reg_type = reg_type;

                Some(r)
            }
            Err(_) => None,
        }
    }

    /// Read a CSR register by its address (delegates to Hart)
    fn read_csr(&self, hart_id: HartId, addr: u64) -> u64 {
        let hart = self.get_hart(hart_id).unwrap();
        hart.read_csr(addr)
    }

    /// Write a value to a CSR register by its address (delegates to Hart)
    fn write_csr(&mut self, hart_id: HartId, addr: u64, value: u64) {
        let hart = self.get_hart_mut(hart_id).unwrap();
        hart.write_csr(addr, value);
    }
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}
