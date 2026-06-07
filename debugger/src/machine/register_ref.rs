#[derive(Debug, Clone, Copy)]
pub enum RegisterType {
    Integer,
    Float,
    Vector,
    Csr,
}

#[derive(Debug, Clone)]
pub struct RegisterRef {
    pub reg_type: RegisterType,

    pub index: usize,
}