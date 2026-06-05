#[derive(Clone, Debug)]
pub struct RegValue<T> {
    pub value: T,
    pub provenance: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct IntegerRegisterFile {
    pub regs: Vec<RegValue<u64>>,
}

#[derive(Clone, Debug)]
pub struct FloatRegisterFile {
    pub regs: Vec<RegValue<f64>>,
}

#[derive(Clone, Debug)]
pub struct VectorRegister {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct VectorRegisterFile {
    pub regs: Vec<VectorRegister>,
}

#[derive(Clone, Debug)]
pub struct VectorState {
    pub vl: usize,
    pub sew: usize,
    pub lmul: usize,
}

#[derive(Clone, Debug)]
pub struct CsrFile {
    pub mhartid: u64,
    pub vl: u64,
    pub vtype: u64,
}

#[derive(Clone, Debug)]
pub struct Hart {
    pub id: u64,
    pub pc: usize,
    pub x: IntegerRegisterFile,
    pub f: FloatRegisterFile,
    pub v: VectorRegisterFile,
    pub vector_state: VectorState,
    pub csr: CsrFile,
}
