pub struct Memory {
    pub bytes: Vec<u8>,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            bytes: vec![0; 64 * 1024 * 1024],
        }
    }
}

impl Memory {

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_size(
        size: usize,
    ) -> Self {

        Self {
            bytes: vec![0; size],
        }
    }

    //
    // 8-bit
    //
    pub fn read_u8(
        &self,
        addr: usize,
    ) -> u8 {
        self.check_range(addr, 1);
        self.bytes[addr]
    }

    pub fn read_i8(
        &self,
        addr: usize,
    ) -> i8 {
        self.check_range(addr, 1);
        self.bytes[addr] as i8
    }

    pub fn write_u8(
        &mut self,
        addr: usize,
        value: u8,
    ) {
        self.check_range(addr, 1);
        self.bytes[addr] = value;
    }

    //
    // 16-bit
    //

    pub fn read_u16(
        &self,
        addr: usize,
    ) -> u16 {
        self.check_range(addr, 2);
        u16::from_le_bytes([
            self.bytes[addr],
            self.bytes[addr + 1],
        ])
    }

    pub fn read_i16(
        &self,
        addr: usize,
    ) -> i16 {
        self.check_range(addr, 2);
        i16::from_le_bytes([
            self.bytes[addr],
            self.bytes[addr + 1],
        ])
    }

    pub fn write_u16(
        &mut self,
        addr: usize,
        value: u16,
    ) {
        self.check_range(addr, 2);
        let bytes = value.to_le_bytes();

        self.bytes[addr..addr + 2]
            .copy_from_slice(&bytes);
    }

    //
    // 32-bit
    //

    pub fn read_u32(
        &self,
        addr: usize,
    ) -> u32 {
        self.check_range(addr, 4);
        u32::from_le_bytes([
            self.bytes[addr],
            self.bytes[addr + 1],
            self.bytes[addr + 2],
            self.bytes[addr + 3],
        ])
    }

    pub fn read_i32(
        &self,
        addr: usize,
    ) -> i32 {
        self.check_range(addr, 4);
        i32::from_le_bytes([
            self.bytes[addr],
            self.bytes[addr + 1],
            self.bytes[addr + 2],
            self.bytes[addr + 3],
        ])
    }

    pub fn write_u32(
        &mut self,
        addr: usize,
        value: u32,
    ) {
        self.check_range(addr, 4);
        let bytes = value.to_le_bytes();

        self.bytes[addr..addr + 4]
            .copy_from_slice(&bytes);
    }

    //
    // 64-bit
    //

    pub fn read_u64(
        &self,
        addr: usize,
    ) -> u64 {
        self.check_range(addr, 8);
        u64::from_le_bytes([
            self.bytes[addr],
            self.bytes[addr + 1],
            self.bytes[addr + 2],
            self.bytes[addr + 3],
            self.bytes[addr + 4],
            self.bytes[addr + 5],
            self.bytes[addr + 6],
            self.bytes[addr + 7],
        ])
    }

    pub fn read_i64(
        &self,
        addr: usize,
    ) -> i64 {
        self.check_range(addr, 8);
        i64::from_le_bytes([
            self.bytes[addr],
            self.bytes[addr + 1],
            self.bytes[addr + 2],
            self.bytes[addr + 3],
            self.bytes[addr + 4],
            self.bytes[addr + 5],
            self.bytes[addr + 6],
            self.bytes[addr + 7],
        ])
    }

    pub fn write_u64(
        &mut self,
        addr: usize,
        value: u64,
    ) {
        self.check_range(addr, 8);
        let bytes = value.to_le_bytes();

        self.bytes[addr..addr + 8]
            .copy_from_slice(&bytes);
    }

    fn check_range(
        &self,
        addr: usize,
        size: usize,
    ) {
        assert!(
            addr + size <= self.bytes.len(),
            "memory access out of range: addr=0x{:x}, size={}",
            addr,
            size
        );
    }
}