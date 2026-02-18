use chrono::Local;
use clap::Parser;
use ffmpeg_next::log::set_level;
use lazy_static::lazy_static;
use midir::{Ignore, MidiInput, MidiOutput};
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Keycode, Mod};
use sdlrig::appruntime::AppRuntime;
use sdlrig::gfxinfo::{GfxEvent, KeyEvent, LogEvent, MidiEvent};
use sdlrig::gfxruntime::{GfxData, GfxRuntime};
use sdlrig::renderspec::RenderSpec;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, thread};

use sdlrig::gfx_lowlevel::bindings::{
    gfx_lowlevel_gpu_ctx, gfx_lowlevel_gpu_ctx_destroy, gfx_lowlevel_gpu_ctx_finish_frame,
    gfx_lowlevel_gpu_ctx_handle_resize, gfx_lowlevel_gpu_ctx_init,
    gfx_lowlevel_gpu_ctx_start_frame,
};

#[derive(Parser, Debug, Clone)]
#[command(author = "VampireExec", version = "1", about = "visualization tool")]
struct Args {
    #[arg(long, default_value = "540")]
    width: u32,
    #[arg(long, default_value = "960")]
    height: u32,
    #[arg(long)]
    wasm: String,
    #[arg(long, default_value = "24")]
    fps: i64,
    #[arg(long, default_value = "false")]
    dry_run: bool,
    #[arg(long, default_value = "false")]
    show_mix_time: bool,
    #[arg(long, default_value = "/tmp/viz")]
    preopen_dir: String,
    #[arg(long, default_value = "false")]
    shader_debug: bool,
    #[arg(long)]
    midi_port: Vec<String>,
    #[arg(long)]
    midi_output: Vec<String>,
}

// Adding a comment as a test
pub fn main() -> anyhow::Result<()> {
    // Tee stderr so we can consume it programmatically.
    // err_rx will receive chunks of stderr output.
    let (err_tx, err_rx) = channel();
    stderr_capture::tee(err_tx);
    let mut log_lines = String::new();

    set_level(ffmpeg_next::log::Level::Error);
    let args = Args::parse();

    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    // MAIN WINDOW
    let mut window = video_subsystem
        .window("Output", args.width, args.height)
        .vulkan()
        .position(0, 0)
        .build()
        .unwrap();

    let mut lowlevel_ctx = unsafe {
        let raw_window = window.raw();
        let ctx = gfx_lowlevel_gpu_ctx_init(raw_window as *mut _);
        if ctx.is_null() {
            panic!("Failed to initialize lowlevel_ctx");
        }
        ctx
    };
    window.raise();

    let mut midi_devices = HashMap::new();
    {
        let midi_in = MidiInput::new("sdlrig-midi-probe")?;
        let ports = midi_in.ports();
        eprintln!("Available midi ports:");
        for (i, p) in ports.iter().enumerate() {
            eprintln!("{}: {}", i, midi_in.port_name(p)?);
            midi_devices.insert(midi_in.port_name(p)?, i);
        }
    }

    let (midi_tx, midi_rx) = channel();
    let _conns = if !args.midi_port.is_empty() {
        let mut conns = Vec::new();
        for device in args.midi_port {
            if let Some(p) = midi_devices.get(&device) {
                let name = device.clone();
                let mut midi_in = MidiInput::new(&name)?;
                midi_in.ignore(Ignore::None);
                let ports = midi_in.ports();
                let port = ports.get(*p).ok_or(anyhow::anyhow!("Invalid midi port"))?;
                println!("Opening midi port {}", midi_in.port_name(port)?);
                let midi_tx = midi_tx.clone();
                conns.push(midi_in.connect(
                    port,
                    "midir-read-input",
                    move |stamp, message, _| {
                        let key = if message.len() >= 2 { message[1] } else { 0 };
                        let vel = if message.len() >= 3 { message[2] } else { 0 };
                        midi_tx
                            .send(MidiEvent {
                                device: name.clone(),
                                channel: message[0] & 0x0F,
                                kind: message[0] & 0xF0,
                                key: key,
                                velocity: vel,
                                timestamp: stamp as i64,
                            })
                            .unwrap();
                    },
                    (),
                )?);
            } else {
                eprintln!("Midi device {} not found", device);
            }
        }
        conns
    } else {
        println!("Not listening for midi");
        vec![]
    };

    let mut midi_output_devices = HashMap::new();
    let mut midi_outs = HashMap::new();
    if !args.midi_output.is_empty() {
        let midi_out = MidiOutput::new("sdlrig-midi-output-probe")?;
        let ports = midi_out.ports();
        eprintln!("Available midi output ports:");
        for (i, p) in ports.iter().enumerate() {
            let port_name = midi_out.port_name(p)?;
            eprintln!("{}: {}", i, port_name);
            midi_output_devices.insert(port_name, i);
        }

        for device in args.midi_output {
            if let Some(p) = midi_output_devices.get(&device) {
                let midi_out = MidiOutput::new("sdlrig-midi-output")?;
                let ports = midi_out.ports();
                let port = ports
                    .get(*p)
                    .ok_or(anyhow::anyhow!("Invalid midi output port"))?;
                println!("Opening midi output port {}", midi_out.port_name(port)?);
                let conn = midi_out.connect(port, "midir-write-output")?;
                midi_outs.insert(device, conn);
            } else {
                eprintln!("Midi output device {} not found", device);
            }
        }
    }

    let (mut canvas_w, mut canvas_h) = window.size();

    let mut event_pump = sdl_context.event_pump().unwrap();
    let start_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    let frames_per_sec = args.fps;
    let ns_per_frame = 1_000_000_000u128 / frames_per_sec as u128;

    let mut frame = (start_time.as_nanos() / ns_per_frame) as i64;

    let mut loader = RuntimeLoader::new();

    let gfx_runtime = GfxRuntime::new(frames_per_sec, frame - 1);

    loader.start(&args.wasm, &args.preopen_dir, None, args.fps, args.dry_run);
    #[allow(unused)]
    let (mut try_app, mut reloaded) = loader.try_finish(
        true,
        canvas_w,
        canvas_h,
        &gfx_runtime,
        None,
        0,
        args.dry_run,
        lowlevel_ctx,
    );
    let mut last_loaded_wasm = SystemTime::now();

    if args.dry_run {
        return Ok(());
    }
    window.raise();
    let mut reg_events = vec![];

    'running: loop {
        assert_eq!(unsafe { (*lowlevel_ctx).started }, false);
        (try_app, reloaded) = loader.try_finish(
            false,
            canvas_w,
            canvas_h,
            &gfx_runtime,
            try_app.as_ref().and_then(|app| Some(app.clone())),
            frame,
            args.dry_run,
            lowlevel_ctx,
        );

        for evt in midi_rx.try_iter() {
            reg_events.push(GfxEvent::MidiEvent(evt));
        }

        if reloaded {
            reg_events.push(GfxEvent::ReloadEvent());
        }

        lazy_static! {
            static ref ACC: Mod = Mod::RSHIFTMOD | Mod::LSHIFTMOD;
        }

        for event in event_pump.poll_iter() {
            match event {
                Event::Window { win_event, .. } => unsafe {
                    match win_event {
                        WindowEvent::Resized(w, h) => {
                            canvas_w = w as u32;
                            canvas_h = h as u32;
                        }
                        WindowEvent::SizeChanged(w, h) => {
                            canvas_w = w as u32;
                            canvas_h = h as u32;
                        }
                        _ => (),
                    }
                    gfx_lowlevel_gpu_ctx_handle_resize(
                        lowlevel_ctx,
                        canvas_w as i32,
                        canvas_h as i32,
                    );
                },
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                Event::KeyDown {
                    keycode: Some(kc),
                    keymod: km,
                    repeat,
                    timestamp,
                    ..
                } => {
                    let shift = km.contains(Mod::RSHIFTMOD) || km.contains(Mod::LSHIFTMOD);
                    let alt = km.contains(Mod::LALTMOD) || km.contains(Mod::RALTMOD);
                    let ctl = km.contains(Mod::LCTRLMOD) || km.contains(Mod::RCTRLMOD);
                    reg_events.push(GfxEvent::KeyEvent(KeyEvent {
                        key: (kc.into_i32() as u32).into(),
                        shift,
                        alt,
                        ctl,
                        down: true,
                        repeat,
                        timestamp: timestamp as i64,
                    }));
                }
                Event::KeyUp {
                    keycode: Some(kc),
                    keymod: km,
                    repeat,
                    timestamp,
                    ..
                } => {
                    let shift = km.contains(Mod::RSHIFTMOD) || km.contains(Mod::LSHIFTMOD);
                    let alt = km.contains(Mod::LALTMOD) || km.contains(Mod::RALTMOD);
                    let ctl = km.contains(Mod::LCTRLMOD) || km.contains(Mod::RCTRLMOD);
                    reg_events.push(GfxEvent::KeyEvent(KeyEvent {
                        key: (kc.into_i32() as u32).into(),
                        shift,
                        alt,
                        ctl,
                        down: false,
                        repeat,
                        timestamp: timestamp as i64,
                    }));
                }
                _ => (),
            }
        }

        // add loop to consume lines from stderr here
        let mut breaker = 0; // safety breaker to prevent infinite loop in case of issues
        while let Ok(chunk) = err_rx.try_recv() {
            breaker += 1;
            if breaker >= 100000 {
                break;
            }
            if let Ok(s) = String::from_utf8(chunk) {
                log_lines.push_str(&s);
            }
        }

        let mut full_lines = Vec::new();
        // Check if we have any newlines to process
        if let Some(last_newline) = log_lines.rfind('\n') {
            let (complete, partial) = log_lines.split_at(last_newline + 1);
            for line in complete.lines() {
                if !line.is_empty() {
                    full_lines.push(line.to_string());
                }
            }
            log_lines = partial.to_string();
        }

        for line in full_lines {
            reg_events.push(GfxEvent::LogEvent(LogEvent {
                message: line.to_string(),
            }));
        }

        if let Some(app_runtime) = try_app.as_ref() {
            let mut specs = match app_runtime.calc(
                canvas_w,
                canvas_h,
                frame,
                gfx_runtime.frames_per_sec,
                &reg_events,
            ) {
                Ok(specs) => specs,
                Err(e) => {
                    eprintln!("Error calculating {:?}", e);
                    try_app.take();
                    continue;
                }
            };

            reg_events.clear();

            unsafe {
                if !gfx_lowlevel_gpu_ctx_start_frame(lowlevel_ctx) {
                    eprintln!("Failed to start frame looping");
                    continue 'running;
                }

                gfx_runtime.reset_mix_dispatches()?;
            }

            for spec in specs.drain(..) {
                if let RenderSpec::SendMidi(cmd) = &spec {
                    let mut bytes: [u8; 3] = [0; 3]; // Placeholder for actual MIDI message bytes
                    bytes[0] = (cmd.event.kind & 0xF0) | (cmd.event.channel & 0x0F);
                    bytes[1] = cmd.event.key;
                    bytes[2] = cmd.event.velocity;
                    if let Some(conn) = midi_outs.get_mut(&cmd.event.device) {
                        conn.send(&bytes).unwrap_or_else(|e| {
                            eprintln!(
                                "failed to send midi message on {}: {}",
                                &cmd.event.device, e
                            )
                        });
                    }
                }

                match gfx_runtime.render(
                    lowlevel_ctx,
                    spec.clone(),
                    frame,
                    args.dry_run,
                    args.shader_debug,
                ) {
                    Err(e) => {
                        eprintln!("Error rendering {:?}", e);
                        try_app.take();
                        break;
                    }
                    _ => (),
                }

                if let RenderSpec::Mix(mix) = &spec {
                    if args.show_mix_time {
                        // let inst = Duration::from_millis(
                        //     (f64::from(gfx_runtime.get_present_time_for_mix(&mix.name)?) * 1000.0)
                        //         as u64,
                        // );
                        // let hours = inst.as_secs() / 60 / 60;
                        // let mins = (inst.as_secs() / 60) % 60;
                        // let secs = inst.as_secs() % 60;
                        // let millis = inst.as_millis() % 1000;
                        // write!(
                        //     &mut hud_text,
                        //     "{}@{hours:0>2}:{mins:0>2}:{secs:0>2}.{millis:0>3}\n",
                        //     mix.name
                        // )?;
                    }
                    for input in &mix.inputs {
                        match input {
                            sdlrig::renderspec::MixInput::Video(v) => {
                                if let Some(event) = gfx_runtime.get_last_frame_event(v)? {
                                    reg_events.push(GfxEvent::FrameEvent(event));
                                }
                            }
                            sdlrig::renderspec::MixInput::Mixed(_) => (),
                        }
                    }
                }
            }
        }

        gfx_runtime.set_last_frame_rendered(frame);
        unsafe {
            match gfx_lowlevel_gpu_ctx_finish_frame(lowlevel_ctx) {
                0 => (),
                err => panic!("Failed to finish frame {}", err),
            }
        }
        // sync video
        let current_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let frames_elapsed = ((current_time.as_nanos() / ns_per_frame) as i64 - frame).max(1);
        frame += frames_elapsed as i64;
        let next_time = Duration::from_nanos(frame as u64 * ns_per_frame as u64);

        if next_time.gt(&current_time) {
            ::std::thread::sleep(next_time.checked_sub(current_time).unwrap());
        }

        if fs::metadata(&args.wasm).unwrap().modified().unwrap() > last_loaded_wasm {
            last_loaded_wasm = SystemTime::now();
            println!("Autoloading wasm at: {}", Local::now().to_rfc3339());
            loader.start(
                &args.wasm,
                &args.preopen_dir,
                try_app.as_ref().and_then(|app| Some(app.clone())),
                args.fps,
                args.dry_run,
            );
        }
    }

    //cleanup
    if let Some(app) = try_app.take() {
        drop(app);
    }
    drop(gfx_runtime);
    unsafe {
        gfx_lowlevel_gpu_ctx_destroy((&mut lowlevel_ctx) as *mut *mut gfx_lowlevel_gpu_ctx);
    }
    Ok(())
}

struct RuntimeLoader {
    handle: Option<JoinHandle<(AppRuntime, HashMap<String, GfxData>)>>,
}

impl RuntimeLoader {
    fn new() -> Self {
        Self { handle: None }
    }

    fn start<T: AsRef<Path>>(
        &mut self,
        path: T,
        preopen_dir: T,
        cached: Option<Arc<AppRuntime>>,
        frames_per_second: i64,
        dry_run: bool,
    ) {
        if self.handle.is_some() {
            return;
        }

        let path: PathBuf = PathBuf::from(path.as_ref());
        let preopen_dir: PathBuf = PathBuf::from(preopen_dir.as_ref());
        self.handle = Some(thread::spawn(move || -> _ {
            let cached_assets = cached.as_ref().map(|ar| ar.loaded_asset_info().clone());
            match AppRuntime::load(
                &path,
                &preopen_dir,
                cached_assets.as_ref().map(|ca| ca.as_ref()),
                frames_per_second,
                dry_run,
            ) {
                Ok((app, loaded_gfx_data)) => {
                    println!("Built at: {}", Local::now().to_rfc3339());
                    (app, loaded_gfx_data)
                }
                Err(e) => panic!("{}", e),
            }
        }));
    }

    fn try_finish(
        &mut self,
        block: bool,
        canvas_w: u32,
        canvas_h: u32,
        gfx_runtime: &GfxRuntime,
        try_app: Option<Arc<AppRuntime>>,
        _frame: i64,
        dry_run: bool,
        _lowlevel_ctx: *mut gfx_lowlevel_gpu_ctx,
    ) -> (Option<Arc<AppRuntime>>, bool) {
        if self.handle.is_none() {
            return (try_app, false);
        }

        if !block && !self.handle.as_ref().unwrap().is_finished() {
            return (try_app, false);
        }

        let (app, mut loaded_gfx_data) = match self.handle.take().unwrap().join() {
            Ok(result) => result,
            Err(e) => {
                let msg = format!("Failed to finish loading: {:?}", e);
                if dry_run {
                    panic!("{}", msg);
                } else {
                    eprintln!("{}", msg);
                }
                return (try_app, false);
            }
        };

        //restore settings if possible
        if let Some(previous) = try_app.as_ref() {
            match previous.extract_settings() {
                Err(e) => eprintln!("Error extracting: {}", e),
                Ok(extracted) => {
                    eprintln!("Extracted {} settings bytes", extracted.len());
                    match app.import_settings(&extracted) {
                        Err(e) => eprintln!("Error restoring settings: {}", e),
                        Ok(_) => eprintln!("completed settings import"),
                    }
                }
            }
        }

        let mut to_remove = gfx_runtime
            .gfx_info()
            .keys()
            .cloned()
            .collect::<HashSet<_>>();

        for (asset, _) in app.loaded_asset_info().as_ref() {
            to_remove.remove(asset.name());
        }

        for (_, gfx_data) in loaded_gfx_data.drain() {
            gfx_runtime.add(gfx_data.info(), gfx_data)
        }

        for k in to_remove {
            if let Err(e) = gfx_runtime.remove(&k) {
                eprintln!("Error removing {}: {}", k, e);
            };
        }

        //dry run calc
        match app.calc(canvas_w, canvas_h, 1, gfx_runtime.frames_per_sec, &vec![]) {
            Ok(mut _specs) => {
                eprintln!("Load complete at {}", Local::now().to_rfc3339());
                // for spec in specs.drain(..) {
                //     match gfx_runtime.render(lowlevel_ctx as *mut _, spec, frame, true) { //always dry run
                //         Err(e) => {
                //             eprintln!("Error rendering {:?}", e);
                //             return (try_app, false);
                //         }
                //         _ => (),
                //     }
                // }
                (Some(Arc::new(app)), true)
            }
            Err(e) => {
                eprintln!("Issue  dry running calculate {}", e);

                return (try_app, false);
            }
        }
    }
}

mod stderr_capture {
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::unix::io::FromRawFd;
    use std::sync::mpsc::Sender;

    pub fn tee(tx: Sender<Vec<u8>>) {
        let mut fds: [libc::c_int; 2] = [0; 2];
        unsafe {
            if libc::pipe(fds.as_mut_ptr()) < 0 {
                eprintln!("Failed to create pipe for stderr tee");
                return;
            }
        }

        let stderr_fd = libc::STDERR_FILENO;
        let orig_stderr_fd = unsafe { libc::dup(stderr_fd) };
        if orig_stderr_fd < 0 {
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            return;
        }

        // Disable buffering on stderr to avoid latency
        unsafe {
            // macOS specific symbol for stderr
            #[cfg(target_os = "macos")]
            {
                extern "C" {
                    static mut __stderrp: *mut libc::FILE;
                }
                libc::setvbuf(__stderrp, std::ptr::null_mut(), libc::_IONBF, 0);
            }
        }

        unsafe {
            libc::dup2(fds[1], stderr_fd);
            libc::close(fds[1]);
        }

        std::thread::spawn(move || {
            let mut reader = unsafe { File::from_raw_fd(fds[0]) };
            let mut writer = unsafe { File::from_raw_fd(orig_stderr_fd) };
            let mut buffer = [0u8; 1024];

            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = buffer[0..n].to_vec();
                        // Ignore errors on original stderr
                        let _ = writer.write_all(&chunk);
                        let _ = writer.flush();

                        // Send to channel
                        if tx.send(chunk).is_err() {
                            break; // Receiver dropped
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
}
