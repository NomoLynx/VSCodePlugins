#[derive(Debug, Clone)]
pub enum RuntimeValue {
    Integer(u64),

    Float32(f32),
    Float64(f64),

    Vector(Vec<u8>),
}

impl From<u64> for RuntimeValue {
    fn from(value: u64) -> Self {
        RuntimeValue::Integer(value)
    }
}

impl From<f32> for RuntimeValue {
    fn from(value: f32) -> Self {
        RuntimeValue::Float32(value)
    }
}

impl From<f64> for RuntimeValue {
    fn from(value: f64) -> Self {
        RuntimeValue::Float64(value)
    }
}

impl From<Vec<u8>> for RuntimeValue {
    fn from(value: Vec<u8>) -> Self {
        RuntimeValue::Vector(value)
    }
}