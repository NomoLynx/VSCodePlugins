use crate::machine::{machine::ProgramId, register_ref::{RegisterRef, RegisterType}, runtime_value::RuntimeValue};

#[derive(Clone, Debug)]
pub struct RegValue<T> {
    pub value: T,
    pub provenance: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct IntegerRegisterFile {
    pub regs: Vec<RegValue<u64>>,
}

impl IntegerRegisterFile {
    pub fn read(&self, reg: usize) -> u64 {
        self.regs[reg].value
    }

    pub fn write(&mut self, reg: usize, value: u64) {
        if reg == 0 {
            return; // x0 is hardwired to 0
        }

        self.regs[reg].value = value;
    }

    pub fn new(size:usize) -> Self {
        Self {
            regs: vec![RegValue { value: 0, provenance: None }; size],
        }
    }
}

impl Default for IntegerRegisterFile {
    fn default() -> Self {
        Self::new(32)
    }
}

#[derive(Clone, Debug)]
pub struct FloatRegisterFile {
    pub regs: Vec<RegValue<f64>>,
}

impl FloatRegisterFile {
    pub fn read(&self, reg: usize) -> f64 {
        self.regs[reg].value
    }

    pub fn write(&mut self, reg: usize, value: f64) {
        self.regs[reg].value = value;
    }

    pub fn new(size:usize) -> Self {
        Self {
            regs: vec![RegValue { value: 0.0, provenance: None }; size],
        }
    }
}

impl Default for FloatRegisterFile {
    fn default() -> Self {
        Self::new(32)
    }
}

#[derive(Clone, Debug)]
pub struct VectorRegister {
    pub bytes: Vec<u8>,
}

impl Default for VectorRegister {
    fn default() -> Self {
        Self {
            bytes: vec![0; 64], // 512 bits = 64 bytes
        }
    }
}

#[derive(Clone, Debug)]
pub struct VectorRegisterFile {
    pub regs: Vec<VectorRegister>,
}

impl Default for VectorRegisterFile {
    fn default() -> Self {
        Self {
            regs: vec![VectorRegister::default(); 32],
        }
    }
}

#[derive(Clone, Debug)]
pub struct VectorState {
    pub vl: usize,
    pub sew: usize,
    pub lmul: usize,
}

impl Default for VectorState {
    fn default() -> Self {
        Self {
            vl: 0,
            sew: 0,
            lmul: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CsrFile {
    pub mhartid: u64,
    pub vl: u64,
    pub vtype: u64,
}

impl Default for CsrFile {
    fn default() -> Self {
        Self {
            mhartid: 0,
            vl: 0,
            vtype: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Hart {
    pub id: u64,
    pub pc: usize,
    pub program_id: ProgramId,
    pub x: IntegerRegisterFile,
    pub f: FloatRegisterFile,
    pub v: VectorRegisterFile,
    pub vector_state: VectorState,
    pub csr: CsrFile,
}

impl Hart {
    pub fn new(id: u64, program_id: ProgramId) -> Self {
        Self {
            id,
            pc: 0,
            program_id,
            x: IntegerRegisterFile::default(),
            f: FloatRegisterFile::default(),
            v: VectorRegisterFile::default(),
            vector_state: VectorState::default(),
            csr: CsrFile::default(),
        }
    }

    pub fn next_pc(&mut self) {
        self.pc += 1;
    }

    pub fn read_register(
        &self,
        reg: &RegisterRef,
    ) -> RuntimeValue {

        match reg.reg_type {

            RegisterType::Integer => {

                RuntimeValue::Integer(
                    self.x.regs[reg.index].value
                )
            }

            RegisterType::Float => {

                RuntimeValue::Float64(
                    self.f.regs[reg.index].value
                )
            }

            RegisterType::Vector => {

                RuntimeValue::Vector(
                    self.v.regs[reg.index]
                        .bytes
                        .clone()
                )
            }

            RegisterType::Csr => {
                todo!()
            }
        }
    }

    pub fn write_register(
        &mut self,
        reg: &RegisterRef,
        value: RuntimeValue,
    ) {

        match (
            reg.reg_type,
            value,
        ) {

            (
                RegisterType::Integer,
                RuntimeValue::Integer(v),
            ) => {

                if reg.index != 0 {
                    self.x.regs[reg.index].value = v;
                }
            }

            (
                RegisterType::Float,
                RuntimeValue::Float64(v),
            ) => {

                self.f.regs[reg.index].value = v;
            }

            (
                RegisterType::Vector,
                RuntimeValue::Vector(v),
            ) => {

                self.v.regs[reg.index].bytes = v;
            }

            _ => {
                panic!("register type mismatch");
            }
        }
    }
}

impl Default for Hart {
    fn default() -> Self {
        Self {
            id: 0,
            pc: 0,
            program_id: 0,
            x: IntegerRegisterFile::default(),
            f: FloatRegisterFile::default(),
            v: VectorRegisterFile::default(),
            vector_state: VectorState::default(),
            csr: CsrFile::default(),
        }
    }
}
