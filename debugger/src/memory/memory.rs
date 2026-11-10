//! Byte-addressable memory composed of one or more [`MemoryRegion`]s.

use crate::debugger_error::{AppResult, DebuggerError};
use crate::memory::memory_region::MemoryRegion;

/// A byte-addressable address space backed by one or more memory regions.
#[derive(Debug)]
pub struct Memory {
    /// The memory regions backing this address space, in allocation order.
    pub(crate) regions: Vec<MemoryRegion>,
}

impl Default for Memory {
    /// Creates a memory with a single default 64 MiB region at base 0.
    fn default() -> Self {
        Self {
            regions: vec![MemoryRegion::default()],
        }
    }
}

impl Memory {
    /// Creates a memory with a single default region.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a memory with a single zeroed region of `size` bytes at base 0.
    #[must_use]
    pub fn with_size(size: usize) -> Self {
        Self {
            regions: vec![MemoryRegion {
                base: 0,
                bytes: vec![0; size],
            }],
        }
    }

    /// Creates a memory with a single zeroed region starting at `base`.
    #[must_use]
    pub fn with_ram(base: u64, size: usize) -> Self {
        Self {
            regions: vec![MemoryRegion {
                base,
                bytes: vec![0; size],
            }],
        }
    }

    /// Reads an unsigned byte at `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn read_u8(&self, addr: u64) -> AppResult<u8> {
        Ok(self.read_le::<1>(addr)?[0])
    }

    /// Reads a signed byte at `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn read_i8(&self, addr: u64) -> AppResult<i8> {
        Ok(self.read_u8(addr)? as i8)
    }

    /// Writes an unsigned byte to `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn write_u8(&mut self, addr: u64, value: u8) -> AppResult<()> {
        self.write_le(addr, [value])
    }

    /// Writes a byte slice to consecutive addresses starting at `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if the whole slice is not
    /// mapped.
    pub fn write_bytes(&mut self, addr: u64, values: &[u8]) -> AppResult<()> {
        let region = self.find_region_mut(addr, values.len())?;
        let offset = region.offset(addr);
        region.bytes[offset..offset + values.len()].copy_from_slice(values);
        Ok(())
    }

    /// Reads an unsigned 16-bit little-endian value at `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn read_u16(&self, addr: u64) -> AppResult<u16> {
        Ok(u16::from_le_bytes(self.read_le(addr)?))
    }

    /// Reads a signed 16-bit little-endian value at `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn read_i16(&self, addr: u64) -> AppResult<i16> {
        Ok(i16::from_le_bytes(self.read_le(addr)?))
    }

    /// Writes an unsigned 16-bit little-endian value to `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn write_u16(&mut self, addr: u64, value: u16) -> AppResult<()> {
        self.write_le(addr, value.to_le_bytes())
    }

    /// Reads an unsigned 32-bit little-endian value at `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn read_u32(&self, addr: u64) -> AppResult<u32> {
        Ok(u32::from_le_bytes(self.read_le(addr)?))
    }

    /// Reads a signed 32-bit little-endian value at `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn read_i32(&self, addr: u64) -> AppResult<i32> {
        Ok(i32::from_le_bytes(self.read_le(addr)?))
    }

    /// Writes an unsigned 32-bit little-endian value to `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn write_u32(&mut self, addr: u64, value: u32) -> AppResult<()> {
        self.write_le(addr, value.to_le_bytes())
    }

    /// Reads an unsigned 64-bit little-endian value at `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn read_u64(&self, addr: u64) -> AppResult<u64> {
        Ok(u64::from_le_bytes(self.read_le(addr)?))
    }

    /// Reads a signed 64-bit little-endian value at `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn read_i64(&self, addr: u64) -> AppResult<i64> {
        Ok(i64::from_le_bytes(self.read_le(addr)?))
    }

    /// Writes an unsigned 64-bit little-endian value to `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    pub fn write_u64(&mut self, addr: u64, value: u64) -> AppResult<()> {
        self.write_le(addr, value.to_le_bytes())
    }

    /// Reads `N` little-endian bytes at `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    fn read_le<const N: usize>(&self, addr: u64) -> AppResult<[u8; N]> {
        let region = self.find_region(addr, N)?;
        let offset = region.offset(addr);
        let mut buf = [0u8; N];
        buf.copy_from_slice(&region.bytes[offset..offset + N]);
        Ok(buf)
    }

    /// Writes `N` little-endian bytes to `addr`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if `addr` is not mapped.
    fn write_le<const N: usize>(&mut self, addr: u64, bytes: [u8; N]) -> AppResult<()> {
        let region = self.find_region_mut(addr, N)?;
        let offset = region.offset(addr);
        region.bytes[offset..offset + N].copy_from_slice(&bytes);
        Ok(())
    }

    /// Finds the region containing `[addr, addr + size)`.
    ///
    /// # Errors
    ///
    /// Returns [`DebuggerError::MemoryOutOfRange`] if no region contains the
    /// range.
    fn find_region(&self, addr: u64, size: usize) -> AppResult<&MemoryRegion> {
        self.regions
            .iter()
            .find(|r| r.contains(addr, size))
            .ok_or(DebuggerError::MemoryOutOfRange { addr, size })
    }

    /// Mutable variant of [`Memory::find_region`].
    fn find_region_mut(&mut self, addr: u64, size: usize) -> AppResult<&mut MemoryRegion> {
        self.regions
            .iter_mut()
            .find(|r| r.contains(addr, size))
            .ok_or(DebuggerError::MemoryOutOfRange { addr, size })
    }
}
