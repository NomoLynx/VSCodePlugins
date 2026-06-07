pub struct MemoryRegion {
    pub base: u64,

    pub bytes: Vec<u8>,
}

impl Default for MemoryRegion {
    fn default() -> Self {
        Self {
            base: 0,
            bytes: vec![0; 64 * 1024 * 1024],
        }
    }
}

impl MemoryRegion {

    pub fn contains(
        &self,
        addr: u64,
        size: usize,
    ) -> bool {

        let end =
            addr + size as u64;

        addr >= self.base
            &&
        end <= self.base + self.bytes.len() as u64
    }

    pub fn offset(
        &self,
        addr: u64,
    ) -> usize {

        (addr - self.base) as usize
    }
}