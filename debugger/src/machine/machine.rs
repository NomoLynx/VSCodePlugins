
use riscv_asm_lib::r5asm::asm_program::AsmProgram;
use riscv_asm_lib::r5asm::imm::Imm::{self};
use riscv_asm_lib::r5asm::instruction::Instruction;
use riscv_asm_lib::r5asm::opcode::OpCode;
use riscv_asm_lib::r5asm::register::Register;

use crate::debugger_error::DebuggerError;
use crate::machine::hart::{Hart, HartPerformance, HartState, PC_INCREMENT};
use crate::machine::processor::Processor;
use crate::machine::register_ref::{RegisterRef, RegisterType};
use crate::memory::memory::Memory;

use std::collections::HashMap;

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

    fn xreg(
        &self,
        name: &Option<String>,
    ) -> Result<usize, DebuggerError> {
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

    fn execute_inst(&mut self, hart_id: HartId, inst: &Instruction) -> Result<(), DebuggerError> {
        let opcode = inst.get_op_code()
                    .map_err(|x| DebuggerError::GeneralError(x.get_error_message()))?;

        let proc_idx = self.get_processor_index(hart_id)
            .ok_or_else(|| DebuggerError::HartNotFound { hart_id })?;

        match opcode {
            OpCode::Add => self.binary_operation(hart_id, &inst, u64::wrapping_add)?,
            OpCode::Sub => self.binary_operation(hart_id, &inst, u64::wrapping_sub)?,
            OpCode::And => self.binary_operation(hart_id, &inst, |a, b| a & b)?,
            OpCode::Or  => self.binary_operation(hart_id, &inst, |a, b| a | b)?,
            OpCode::Xor => self.binary_operation(hart_id, &inst, |a, b| a ^ b)?,
            OpCode::Sll => self.binary_operation(hart_id, &inst, |a, b| a << (b & 0x3f) as u32)?,
            OpCode::Srl => self.binary_operation(hart_id, &inst, |a, b| a >> (b & 0x3f) as u32)?,
            OpCode::Sra => self.binary_operation(hart_id, &inst, |a, b| ((a as i64) >> (b & 0x3f) as u32) as u64)?,
            OpCode::Slt  => self.binary_operation(hart_id, &inst, |a, b| ((a as i64) < (b as i64)) as u64)?,
            OpCode::Sltu => self.binary_operation(hart_id, &inst, |a, b| (a < b) as u64)?,
            OpCode::Mul  => self.binary_operation(hart_id, &inst, |a, b| a.wrapping_mul(b))?,
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
        let err_msg = format!("invalid hart: {}", hart_id);
        let hart = self.get_hart_mut(hart_id).ok_or_else(|| DebuggerError::GeneralError(err_msg))?;
        hart.next_pc();
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
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}
