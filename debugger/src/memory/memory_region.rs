//! A single contiguous, base-addressed block of addressable memory.

/// A contiguous, base-addressed block of addressable memory.
#[derive(Debug, Clone)]
pub(crate) struct MemoryRegion {
    /// Base address of this region in the address space.
    pub(crate) base: u64,
    /// Raw byte storage backing this region.
    pub(crate) bytes: Vec<u8>,
}

impl Default for MemoryRegion {
    /// Creates a 64 MiB region starting at address 0.
    fn default() -> Self {
        Self {
            base: 0,
            bytes: vec![0; 64 * 1024 * 1024],
        }
    }
}

impl MemoryRegion {
    /// Returns `true` if `[addr, addr + size)` lies entirely within this region.
    pub(crate) fn contains(&self, addr: u64, size: usize) -> bool {
        let Some(end) = addr.checked_add(size as u64) else {
            return false; // overflow → cannot be contained
        };
        let Some(region_end) = self.base.checked_add(self.bytes.len() as u64) else {
            return false;
        };
        addr >= self.base && end <= region_end
    }

    /// Converts an absolute address to a byte offset within this region.
    ///
    /// # Panics
    ///
    /// Panics if `addr < self.base`. Callers must ensure the address lies
    /// within the region (e.g. via [`MemoryRegion::contains`]) first.
    pub(crate) fn offset(&self, addr: u64) -> usize {
        debug_assert!(addr >= self.base, "address 0x{addr:x} below region base 0x{:x}", self.base);
        (addr - self.base) as usize
    }
}
