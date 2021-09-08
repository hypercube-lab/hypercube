 

use std::sync::{Once, ONCE_INIT};
extern crate env_logger;

static INIT: Once = ONCE_INIT;

 
pub fn setup() {
    INIT.call_once(|| {
        env_logger::Builder::from_default_env()
            .default_format_timestamp_nanos(true)
            .init();
    });
}
