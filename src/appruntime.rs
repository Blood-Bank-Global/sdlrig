use std::{
    collections::HashMap,
    error::Error,
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{bail, Result};
use wasmtime::{Caller, Config, Engine, Extern, Instance, Linker, Module, Store, TypedFunc};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::{
    p1::{self, WasiP1Ctx},
    DirPerms, FilePerms,
};

use crate::{
    gfxinfo::{Asset, GfxEvent, GfxInfo},
    gfxruntime,
    renderspec::RenderCalcErr,
};
use crate::{gfxruntime::GfxData, renderspec::RenderSpec};

pub struct AppRuntime {
    _engine: Engine,
    _linker: Linker<WasiP1Ctx>,
    buf_ref: Arc<Mutex<Vec<u8>>>,
    reg_events_ref: Arc<Mutex<Vec<u8>>>,
    loaded_asset_info_ref: Arc<HashMap<Asset, GfxInfo>>,
    settings_ref: Arc<Mutex<Vec<u8>>>,
    store: Arc<Mutex<Store<WasiP1Ctx>>>,
    _module: Module,
    _instance: Instance,
    calc_fn: TypedFunc<(u32, u32, i64, i64), u32>,
    save_settings_fn: TypedFunc<(), ()>,
    restore_settings_fn: TypedFunc<(), ()>,
}

impl AppRuntime {
    pub fn load<P: AsRef<Path>>(
        path: P,
        preopen: P,
        cached: Option<&HashMap<Asset, GfxInfo>>,
        frames_per_second: i64,
        dry_run: bool,
    ) -> Result<(Self, HashMap<String, GfxData>)> {
        // Define the WASI functions globally on the `Config`.
        let config = Config::new();
        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);
        p1::add_to_linker_sync(&mut linker, |s| s)?;

        let buf_ref: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(vec![]));
        let guest_buf_ref = buf_ref.clone();

        linker.func_wrap(
            "host",
            "send_bytes",
            move |mut caller: Caller<'_, WasiP1Ctx>, ptr: u32, len: u32| {
                let mem = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => panic!("failed to get memory"),
                };
                let offset = ptr as usize;
                let buf_ref = guest_buf_ref.clone();
                let mut lock = buf_ref.lock();
                let buf = lock.as_deref_mut().unwrap();

                buf.resize(len as usize, Default::default());
                mem.read(&caller, offset, buf.as_mut_slice()).unwrap();
            },
        )?;

        let settings_ref = Arc::new(Mutex::new(vec![]));
        let guest_buf_ref = settings_ref.clone();

        linker.func_wrap(
            "host",
            "send_settings",
            move |mut caller: Caller<'_, WasiP1Ctx>, ptr: u32, len: u32| {
                let mem = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => panic!("failed to get memory"),
                };
                let offset = ptr as usize;
                let buf_ref = guest_buf_ref.clone();
                let mut lock = buf_ref.lock();
                let buf = lock.as_deref_mut().unwrap();

                buf.resize(len as usize, Default::default());
                mem.read(&caller, offset, buf.as_mut_slice()).unwrap();
            },
        )?;

        let guest_buf_ref = settings_ref.clone();

        linker.func_wrap(
            "host",
            "recv_settings_size",
            move |_: Caller<'_, WasiP1Ctx>| -> u64 {
                let buf_ref = guest_buf_ref.clone();
                let mut lock = buf_ref.lock();
                let buf = lock.as_deref_mut().unwrap();
                buf.len() as u64
            },
        )?;

        let guest_buf_ref = settings_ref.clone();
        linker.func_wrap(
            "host",
            "recv_settings",
            move |mut caller: Caller<'_, WasiP1Ctx>, ptr: u32| {
                let mem = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => panic!("failed to get memory"),
                };
                let offset = ptr as usize;
                let buf_ref = guest_buf_ref.clone();
                let mut lock = buf_ref.lock();
                let buf = lock.as_deref_mut().unwrap();
                mem.write(caller, offset, buf.as_mut_slice()).unwrap();
            },
        )?;

        let gfx_info_ref = Arc::new(Mutex::new(Vec::<u8>::new()));
        let guest_gfx_info_ref = gfx_info_ref.clone();

        linker.func_wrap(
            "host",
            "recv_gfx_info",
            move |mut caller: Caller<'_, WasiP1Ctx>, ptr: u32| {
                let mem = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => panic!("failed to get memory"),
                };
                let offset = ptr as usize;
                let buf_ref = guest_gfx_info_ref.clone();
                let mut lock = buf_ref.lock();
                let buf = lock.as_deref_mut().unwrap();

                mem.write(caller, offset, buf.as_mut_slice()).unwrap();
            },
        )?;

        let guest_gfx_info_ref = gfx_info_ref.clone();
        linker.func_wrap(
            "host",
            "gfx_info_serialized_size",
            move |_: Caller<'_, WasiP1Ctx>| -> u32 {
                guest_gfx_info_ref.clone().lock().unwrap().len() as u32
            },
        )?;

        let reg_events_ref = Arc::new(Mutex::new(Vec::<u8>::new()));
        let guest_reg_events_ref = reg_events_ref.clone();
        linker.func_wrap(
            "host",
            "recv_reg_events",
            move |mut caller: Caller<'_, WasiP1Ctx>, ptr: u32| {
                let mem = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => panic!("failed to get memory"),
                };
                let offset = ptr as usize;
                let buf_ref = guest_reg_events_ref.clone();
                let mut lock = buf_ref.lock();
                let buf = lock.as_deref_mut().unwrap();

                mem.write(caller, offset, buf.as_mut_slice()).unwrap();
            },
        )?;

        let guest_reg_events_ref = reg_events_ref.clone();
        linker.func_wrap(
            "host",
            "reg_events_serialized_size",
            move |_: Caller<'_, WasiP1Ctx>| -> u32 {
                guest_reg_events_ref.clone().lock().unwrap().len() as u32
            },
        )?;

        let wasi = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_args()
            .preopened_dir(preopen, "/tmp/viz", DirPerms::all(), FilePerms::all())
            .expect("Issue with preopening dir")
            .build_p1();

        let mut store = Store::<WasiP1Ctx>::new(&engine, wasi);

        // Instantiate our module with the imports we've created, and run it.
        let module = Module::from_file(&engine, path)?;
        linker.module(&mut store, "", &module)?;
        let instance = linker.instantiate(&mut store, &module)?;
        let calc_fn = instance
            .get_typed_func::<(u32, u32, i64, i64), u32>(&mut store, "calculate_internal")?;
        let asset_list_fn =
            instance.get_typed_func::<(i64,), u32>(&mut store, "asset_list_internal")?;

        let save_settings_fn = instance.get_typed_func::<(), ()>(&mut store, "save_settings")?;
        let restore_settings_fn =
            instance.get_typed_func::<(), ()>(&mut store, "restore_settings")?;

        let asset_ref = Arc::<HashMap<String, Asset>>::new({
            // load in the textures
            match RenderCalcErr::from(asset_list_fn.call(&mut store, (frames_per_second,))? as u8) {
                RenderCalcErr::None => (),
                _ => bail!("Got error getting tex list"),
            }

            let asset_list_ref_clone = buf_ref.clone();
            let lock = asset_list_ref_clone.lock().unwrap();
            let returned_asset_list_buf = lock.as_slice();
            let mut asset_list = serde_json::from_slice::<Vec<Asset>>(returned_asset_list_buf)?;
            HashMap::from_iter(asset_list.drain(..).map(|a| (String::from(a.name()), a)))
        });

        let mut loaded_asset_info = HashMap::new();
        let mut gfx_info_map = HashMap::new();
        let mut gfx_data_map = HashMap::new();
        for (name, asset) in asset_ref.iter() {
            if let Some(Some(info)) = cached.as_ref().map(|c| c.get(asset)) {
                gfx_info_map.insert(name.clone(), info.clone());
                loaded_asset_info.insert(asset.clone(), info.clone());
            } else {
                let gfx_data = match gfxruntime::load(asset) {
                    Ok(gfx_data) => gfx_data,
                    Err(e) => {
                        let msg = format!("Error loading in app runtime {:?} {}", asset, e);
                        if dry_run {
                            bail!("{}", msg);
                        } else {
                            println!("{}", msg);
                            continue;
                        }
                    }
                };

                gfx_info_map.insert(gfx_data.name(), gfx_data.info());
                loaded_asset_info.insert(asset.clone(), gfx_data.info());
                gfx_data_map.insert(gfx_data.name(), gfx_data);
            }
        }

        {
            // share back tex info
            let Ok(serialized) = serde_json::to_string(&gfx_info_map) else {
                bail!("Faied to serialize gfx info map");
            };
            let mut gfx_info_buf = gfx_info_ref.lock().unwrap();
            gfx_info_buf.resize(serialized.as_bytes().len(), 0);
            gfx_info_buf.clone_from_slice(serialized.as_bytes());
        }

        Ok((
            Self {
                _engine: engine,
                _linker: linker,
                buf_ref,
                reg_events_ref,
                settings_ref,
                loaded_asset_info_ref: Arc::new(loaded_asset_info),
                store: Arc::new(Mutex::new(store)),
                _module: module,
                _instance: instance,
                calc_fn,
                save_settings_fn,
                restore_settings_fn,
            },
            gfx_data_map,
        ))
    }

    pub fn calc(
        &self,
        canvas_w: u32,
        canvas_h: u32,
        frame: i64,
        fps: i64,
        reg_events: &[GfxEvent],
    ) -> Result<Vec<RenderSpec>, Box<dyn Error>> {
        {
            {
                let Ok(mut reg_lock) = self.reg_events_ref.lock() else {
                    return Err("Reg events array is poisoned".into());
                };

                reg_lock.clear();
                reg_lock.append(&mut serde_json::to_vec(reg_events)?);
            }

            let mut lock = self.store.lock();
            let store = lock.as_deref_mut().unwrap();

            let err = RenderCalcErr::from(
                self.calc_fn.call(store, (canvas_w, canvas_h, frame, fps))? as u8,
            );

            match err {
                RenderCalcErr::None => (),
                _ => return Err("Got issue from wasm".into()),
            }
        }

        let lock = self.buf_ref.lock().unwrap();
        let specs = lock.as_slice();

        Ok(serde_json::from_slice(specs)?)
    }

    pub fn loaded_asset_info(&self) -> Arc<HashMap<Asset, GfxInfo>> {
        self.loaded_asset_info_ref.clone()
    }

    pub fn extract_settings(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut lock = self.store.lock();
        let store = lock.as_deref_mut().unwrap();

        match self.save_settings_fn.call(store, ()) {
            Err(e) => return Err(format!("Could not save settings {:?}", e).into()),
            Ok(_) => (),
        }

        let settings_lock = self.settings_ref.lock();
        Ok(settings_lock.unwrap().to_vec())
    }

    pub fn import_settings(&self, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
        {
            let mut lock = self.settings_ref.lock();
            let settings = lock.as_deref_mut().unwrap();
            settings.clear();
            settings.extend_from_slice(bytes);
        }
        let mut lock = self.store.lock();
        let store = lock.as_deref_mut().unwrap();

        match self.restore_settings_fn.call(store, ()) {
            Err(e) => Err(format!("Could not restore settings {:?}", e).into()),
            _ => Ok(()),
        }
    }
}
