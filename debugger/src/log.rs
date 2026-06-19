use log::{info, error};

pub fn init_logger() {
    log4rs::init_file("log4rs.yaml", Default::default()).unwrap();
}