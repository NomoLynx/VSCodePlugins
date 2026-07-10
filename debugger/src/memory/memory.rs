use crate::debugger_error::DebuggerError;
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

    pub fn read_u8(&self, addr: u64) -> Result<u8, DebuggerError> {
        let region = self.find_region(addr, 1)?;
        let offset = region.offset(addr);
        Ok(region.bytes[offset])
    }

    pub fn read_i8(&self, addr: u64) -> Result<i8, DebuggerError> {
        let region = self.find_region(addr, 1)?;
        let offset = region.offset(addr);
        Ok(region.bytes[offset] as i8)
    }

    pub fn write_u8(&mut self, addr: u64, value: u8) -> Result<(), DebuggerError> {
        let region = self.find_region_mut(addr, 1)?;
        let offset = region.offset(addr);
        region.bytes[offset] = value;
        Ok(())
    }

    pub fn write_bytes(&mut self, addr: u64, values: &[u8]) -> Result<(), DebuggerError> {
        let region = self.find_region_mut(addr, values.len())?;
        let offset = region.offset(addr);
        region.bytes[offset..offset + values.len()]
            .copy_from_slice(values);
        Ok(())
    }

    //
    // 16-bit
    //

    pub fn read_u16(&self, addr: u64) -> Result<u16, DebuggerError> {
        let region = self.find_region(addr, 2)?;
        let offset = region.offset(addr);
        Ok(u16::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
        ]))
    }

    pub fn read_i16(&self, addr: u64) -> Result<i16, DebuggerError> {
        let region = self.find_region(addr, 2)?;
        let offset = region.offset(addr);
        Ok(i16::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
        ]))
    }

    pub fn write_u16(&mut self, addr: u64, value: u16) -> Result<(), DebuggerError> {
        let region = self.find_region_mut(addr, 2)?;
        let offset = region.offset(addr);
        region.bytes[offset..offset + 2]
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    //
    // 32-bit
    //

    pub fn read_u32(&self, addr: u64) -> Result<u32, DebuggerError> {
        let region = self.find_region(addr, 4)?;
        let offset = region.offset(addr);
        Ok(u32::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
            region.bytes[offset + 2],
            region.bytes[offset + 3],
        ]))
    }

    pub fn read_i32(&self, addr: u64) -> Result<i32, DebuggerError> {
        let region = self.find_region(addr, 4)?;
        let offset = region.offset(addr);
        Ok(i32::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
            region.bytes[offset + 2],
            region.bytes[offset + 3],
        ]))
    }

    pub fn write_u32(&mut self, addr: u64, value: u32) -> Result<(), DebuggerError> {
        let region = self.find_region_mut(addr, 4)?;
        let offset = region.offset(addr);
        region.bytes[offset..offset + 4]
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    //
    // 64-bit
    //

    pub fn read_u64(&self, addr: u64) -> Result<u64, DebuggerError> {
        let region = self.find_region(addr, 8)?;
        let offset = region.offset(addr);
        Ok(u64::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
            region.bytes[offset + 2],
            region.bytes[offset + 3],
            region.bytes[offset + 4],
            region.bytes[offset + 5],
            region.bytes[offset + 6],
            region.bytes[offset + 7],
        ]))
    }

    pub fn read_i64(&self, addr: u64) -> Result<i64, DebuggerError> {
        let region = self.find_region(addr, 8)?;
        let offset = region.offset(addr);
        Ok(i64::from_le_bytes([
            region.bytes[offset],
            region.bytes[offset + 1],
            region.bytes[offset + 2],
            region.bytes[offset + 3],
            region.bytes[offset + 4],
            region.bytes[offset + 5],
            region.bytes[offset + 6],
            region.bytes[offset + 7],
        ]))
    }

    pub fn write_u64(&mut self, addr: u64, value: u64) -> Result<(), DebuggerError> {
        let region = self.find_region_mut(addr, 8)?;
        let offset = region.offset(addr);
        region.bytes[offset..offset + 8]
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn find_region(&self, addr: u64, size: usize) -> Result<&MemoryRegion, DebuggerError> {
        self.regions
            .iter()
            .find(|r| r.contains(addr, size))
            .ok_or(DebuggerError::MemoryOutOfRange { addr, size })
    }

    fn find_region_mut(&mut self, addr: u64, size: usize) -> Result<&mut MemoryRegion, DebuggerError> {
        self.regions
            .iter_mut()
            .find(|r| r.contains(addr, size))
            .ok_or(DebuggerError::MemoryOutOfRange { addr, size })
    }
}