#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused)]
mod _internal {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

use std::{
    ffi::{CStr, CString},
    iter::once,
};

pub fn glob(pattern: &str) -> Option<Vec<String>> {
    let terminated = pattern.bytes().chain(once('\0' as u8)).collect();
    let cpattern = CString::from_vec_with_nul(terminated).unwrap();
    let mut pathv = vec![];

    unsafe {
        let mut res: _internal::glob_t = std::mem::zeroed();
        let err = _internal::glob(
            cpattern.as_ptr(),
            _internal::GLOB_TILDE as i32,
            None,
            &mut res as *mut _,
        );

        if err != 0 {
            _internal::globfree(&mut res as *mut _);
            return None;
        }

        for i in 0..res.gl_matchc {
            let entry = CStr::from_ptr(*(res.gl_pathv.offset(i as isize)));
            pathv.push(entry.to_string_lossy().to_string());
        }

        _internal::globfree(&mut res as *mut _);
    }

    Some(pathv)
}
