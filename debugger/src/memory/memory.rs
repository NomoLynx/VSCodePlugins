use crate::memory::memory_region::MemoryRegion;

pub struct Memory {
    pub regions: Vec<MemoryRegion>,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            regions: vec![MemoryRegion::default()],
        }
    }
}

impl Memory {

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_size(size: usize) -> Self {
        Self {
            regions: vec![
                MemoryRegion {
                    base: 0,
                    bytes: vec![0; size],
                }
            ],
        }
    }

    pub fn with_ram(
        base: u64,
        size: usize,
    ) -> Self {

        Self {
            regions: vec![
                MemoryRegion {
                    base,
                    bytes: vec![0; size],
                }
            ],
        }
    }

    //
    // 8-bit
    //

    pub fn read_u8(
        &self,
        addr: u64,
    ) -> u8 {

        let region =
            self.find_region(addr, 1);

        let offset =
            region.offset(addr);

        region.bytes[offset]
    }

    pub fn read_i8(
        &self,
        addr: u64,
    ) -> i8 {

        let region =
            self.find_region(addr, 1);

        let offset =
            region.offset(addr);

        region.bytes[offset] as i8
    }

    pub fn write_u8(
        &mut self,
        addr: u64,
        value: u8,
    ) {

        let region =
            self.find_region_mut(addr, 1);

        let offset =
            region.offset(addr);

        region.bytes[offset] = value;
    }

    pub fn write_bytes(
        &mut self,
        addr: u64,
        values: &[u8],
    ) {

        let region =
            self.find_region_mut(addr, values.len());

        let offset =
            region.offset(addr);

        region.bytes[offset..offset + values.len()]
            .copy_from_slice(values);
    }

    //
    // 16-bit
    //

    pub fn read_u16(
        &self,
        addr: u64,
    ) -> u16 {

        let region =
            self.find_region(addr, 2);

        let offset =
            region.offset(addr);

        u16::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
        ])
    }

    pub fn read_i16(
        &self,
        addr: u64,
    ) -> i16 {

        let region =
            self.find_region(addr, 2);

        let offset =
            region.offset(addr);

        i16::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
        ])
    }

    pub fn write_u16(
        &mut self,
        addr: u64,
        value: u16,
    ) {

        let region =
            self.find_region_mut(addr, 2);

        let offset =
            region.offset(addr);

        region.bytes[offset..offset + 2]
            .copy_from_slice(
                &value.to_le_bytes()
            );
    }

    //
    // 32-bit
    //

    pub fn read_u32(
        &self,
        addr: u64,
    ) -> u32 {

        let region =
            self.find_region(addr, 4);

        let offset =
            region.offset(addr);

        u32::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
            region.bytes[offset + 2],
            region.bytes[offset + 3],
        ])
    }

    pub fn read_i32(
        &self,
        addr: u64,
    ) -> i32 {

        let region =
            self.find_region(addr, 4);

        let offset =
            region.offset(addr);

        i32::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
            region.bytes[offset + 2],
            region.bytes[offset + 3],
        ])
    }

    pub fn write_u32(
        &mut self,
        addr: u64,
        value: u32,
    ) {

        let region =
            self.find_region_mut(addr, 4);

        let offset =
            region.offset(addr);

        region.bytes[offset..offset + 4]
            .copy_from_slice(
                &value.to_le_bytes()
            );
    }

    //
    // 64-bit
    //

    pub fn read_u64(
        &self,
        addr: u64,
    ) -> u64 {

        let region =
            self.find_region(addr, 8);

        let offset =
            region.offset(addr);

        u64::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
            region.bytes[offset + 2],
            region.bytes[offset + 3],
            region.bytes[offset + 4],
            region.bytes[offset + 5],
            region.bytes[offset + 6],
            region.bytes[offset + 7],
        ])
    }

    pub fn read_i64(
        &self,
        addr: u64,
    ) -> i64 {

        let region =
            self.find_region(addr, 8);

        let offset =
            region.offset(addr);

        i64::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
            region.bytes[offset + 2],
            region.bytes[offset + 3],
            region.bytes[offset + 4],
            region.bytes[offset + 5],
            region.bytes[offset + 6],
            region.bytes[offset + 7],
        ])
    }

    pub fn write_u64(
        &mut self,
        addr: u64,
        value: u64,
    ) {

        let region =
            self.find_region_mut(addr, 8);

        let offset =
            region.offset(addr);

        region.bytes[offset..offset + 8]
            .copy_from_slice(
                &value.to_le_bytes()
            );
    }

    pub fn find_region(
        &self,
        addr: u64,
        size: usize,
    ) -> &MemoryRegion {

        self.regions
            .iter()
            .find(|r| r.contains(addr, size))
            .unwrap_or_else(|| {
                panic!(
                    "memory access out of range: addr=0x{:x}, size={}",
                    addr,
                    size
                )
            })
    }

    pub fn find_region_mut(
        &mut self,
        addr: u64,
        size: usize,
    ) -> &mut MemoryRegion {

        self.regions
            .iter_mut()
            .find(|r| r.contains(addr, size))
            .unwrap_or_else(|| {
                panic!(
                    "memory access out of range: addr=0x{:x}, size={}",
                    addr,
                    size
                )
            })
    }
}