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

    pub fn read_u8(
        &self,
        addr: usize,
    ) -> u8 {

        self.bytes[addr]
    }

    pub fn write_u8(
        &mut self,
        addr: usize,
        value: u8,
    ) {

        self.bytes[addr] = value;
    }

    pub fn read_u32(
        &self,
        addr: usize,
    ) -> u32 {

        let bytes = [
            self.bytes[addr],
            self.bytes[addr + 1],
            self.bytes[addr + 2],
            self.bytes[addr + 3],
        ];

        u32::from_le_bytes(bytes)
    }

    pub fn write_u32(
        &mut self,
        addr: usize,
        value: u32,
    ) {

        let bytes = value.to_le_bytes();

        self.bytes[addr]     = bytes[0];
        self.bytes[addr + 1] = bytes[1];
        self.bytes[addr + 2] = bytes[2];
        self.bytes[addr + 3] = bytes[3];
    }

    pub fn read_u64(
        &self,
        addr: usize,
    ) -> u64 {

        let bytes = [
            self.bytes[addr],
            self.bytes[addr + 1],
            self.bytes[addr + 2],
            self.bytes[addr + 3],
            self.bytes[addr + 4],
            self.bytes[addr + 5],
            self.bytes[addr + 6],
            self.bytes[addr + 7],
        ];

        u64::from_le_bytes(bytes)
    }

    pub fn write_u64(
        &mut self,
        addr: usize,
        value: u64,
    ) {

        let bytes = value.to_le_bytes();

        self.bytes[addr]     = bytes[0];
        self.bytes[addr + 1] = bytes[1];
        self.bytes[addr + 2] = bytes[2];
        self.bytes[addr + 3] = bytes[3];
        self.bytes[addr + 4] = bytes[4];
        self.bytes[addr + 5] = bytes[5];
        self.bytes[addr + 6] = bytes[6];
        self.bytes[addr + 7] = bytes[7];
    }

    pub fn read_i32(
        &self,
        addr: usize,
    ) -> i32 {
        self.read_u32(addr) as i32
    }

    pub fn read_i64(
        &self,
        addr: usize,
    ) -> i64 {
        self.read_u64(addr) as i64
    }
}