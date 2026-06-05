mod machine;
mod memory;
mod program;
mod vm;
mod trace;
mod debug;
mod debugger_error;

use crate::machine::hart::{
    Hart,
    IntegerRegisterFile,
    FloatRegisterFile,
    VectorRegisterFile,
    VectorState,
    CsrFile,
    RegValue,
};

fn main() {
    let mut hart = Hart::default();

    // t1 = 10
    hart.x.write(6, 10);

    // t2 = 20
    hart.x.write(7, 20);
}
