#[derive(Debug, Clone)]
pub enum RuntimeValue {
    Integer(u64),

    Float32(f32),
    Float64(f64),

    Vector(Vec<u8>),
}