#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(non_upper_case_globals)]
pub mod bindings {
    include!(concat!(env!("OUT_DIR"), "/gfx_lowlevel_bindings.rs"));
}
