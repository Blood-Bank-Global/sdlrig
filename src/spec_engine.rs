use std::{
    collections::HashMap,
    error::Error,
    sync::{Mutex, Once},
    u64,
};

use crate::{
    gfxinfo::{Asset, GfxEvent, GfxInfo},
    renderspec::{RenderCalcErr, RenderSpec},
};
use serde_json;

#[link(wasm_import_module = "host")]
unsafe extern "C" {
    fn send_bytes(ptr: u32, len: u32);
    fn send_settings(ptr: u32, len: u32);
    fn recv_settings(pts: u32);
    fn recv_settings_size() -> u64;
    fn recv_gfx_info(ptr: u32);
    fn gfx_info_serialized_size() -> u32;
    fn recv_reg_events(ptr: u32);
    fn reg_events_serialized_size() -> u32;
}

extern "Rust" {
    fn asset_list(fps: i64) -> Vec<Asset>;
    fn calculate(
        canvas_w: u32,
        canvas_h: u32,
        frame: i64,
        fps: i64,
        gfx_info: &HashMap<String, GfxInfo>,
        reg_events: &[GfxEvent],
    ) -> Result<Vec<RenderSpec>, Box<dyn Error>>;
    fn encode_settings() -> Vec<u8>;
    fn decode_settings(bytes: &[u8]);
}

static INITIALIZE: Once = Once::new();
static GFX_INFO: Mutex<Option<HashMap<String, GfxInfo>>> = Mutex::new(None);

#[no_mangle]
pub extern "C" fn asset_list_internal(fps: i64) -> u32 {
    let asset_list = unsafe { asset_list(fps) };

    let v = match serde_json::to_vec(&asset_list) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Err serializing asset data {:?}", e);
            return RenderCalcErr::AssetDataErr as u32;
        }
    };

    unsafe {
        send_bytes(v.as_ptr() as u32, v.len() as u32);
    }
    RenderCalcErr::None as u32
}

#[no_mangle]
pub extern "C" fn calculate_internal(canvas_w: u32, canvas_h: u32, frame: i64, fps: i64) -> u32 {
    INITIALIZE.call_once(|| {
        init_gfx_info();
    });

    let sz = unsafe { reg_events_serialized_size() } as usize;
    let mut buf: Vec<u8> = Vec::with_capacity(sz);
    buf.resize_with(sz, || 0u8);
    unsafe { recv_reg_events(buf.as_mut_ptr() as u32) }
    let reg_events: Vec<GfxEvent> = serde_json::from_slice(buf.as_slice()).unwrap();

    match unsafe {
        calculate(
            canvas_w,
            canvas_h,
            frame,
            fps,
            GFX_INFO.lock().unwrap().as_ref().unwrap(),
            &reg_events,
        )
    } {
        Ok(specs) => {
            let json = serde_json::to_string(&specs).unwrap();
            unsafe {
                send_bytes(
                    json.as_bytes().as_ptr() as u32,
                    json.as_bytes().len() as u32,
                )
            };
            RenderCalcErr::None as u32
        }
        Err(e) => {
            eprintln!(
                "Error calculating {} {} {}: {}",
                canvas_w, canvas_h, frame, e
            );
            RenderCalcErr::Unknown as u32
        }
    }
}

fn init_gfx_info() {
    let mut lock = GFX_INFO.lock().unwrap();
    let sz = unsafe { gfx_info_serialized_size() } as usize;
    let mut buf: Vec<u8> = Vec::with_capacity(sz);
    buf.resize_with(sz, || 0u8);
    unsafe { recv_gfx_info(buf.as_mut_ptr() as u32) }
    let map: HashMap<String, GfxInfo> = serde_json::from_slice(buf.as_slice()).unwrap();

    lock.replace(map);
}

#[no_mangle]
pub extern "C" fn save_settings() {
    unsafe {
        let bytes = encode_settings();
        send_settings(bytes.as_ptr() as u32, bytes.len() as u32)
    };
}

#[no_mangle]
pub extern "C" fn restore_settings() {
    unsafe {
        let sz = recv_settings_size() as usize;
        let mut buf: Vec<u8> = Vec::with_capacity(sz);
        buf.resize_with(sz, || 0u8);
        recv_settings(buf.as_mut_ptr() as u32);
        decode_settings(&buf);
    }
}
