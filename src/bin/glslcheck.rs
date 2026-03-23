use clap::Parser;
use ffmpeg_next::log::set_level;
use sdlrig::gfxinfo::{VidMixer, VidMixerInfo};
use sdlrig::vidruntime::{VidMixerData, VidMixerInput};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use sdlrig::gfx_lowlevel::bindings::gfx_lowlevel_gpu_ctx_init;

#[derive(Parser, Debug, Clone)]
#[command(author = "VampireExec", version = "1", about = "visualization tool")]
struct Args {
    #[arg(long, default_value = "640")]
    width: u32,
    #[arg(long, default_value = "960")]
    height: u32,
    #[arg(long, default_value = "24")]
    fps: i64,
    #[arg(long, default_value = "false")]
    show_mix_time: bool,
    #[arg(long, default_value = "false")]
    shader_debug: bool,
    #[arg(long)]
    shader_path: String,
    #[arg(long)]
    include_path: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    set_level(ffmpeg_next::log::Level::Error);
    let args = Args::parse();

    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    // MAIN WINDOW
    let window = video_subsystem
        .window("Output", args.width, args.height)
        .vulkan()
        .position(0, 0)
        .build()
        .unwrap();

    let lowlevel_ctx = unsafe {
        let raw_window = window.raw();
        let ctx = gfx_lowlevel_gpu_ctx_init(raw_window as *mut _);
        if ctx.is_null() {
            panic!("Failed to initialize lowlevel_ctx");
        }
        ctx
    };
    // window.raise();

    let start_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    let frames_per_sec = args.fps;
    let ns_per_frame = 1_000_000_000u128 / frames_per_sec as u128;
    let frame = (start_time.as_nanos() / ns_per_frame) as i64;

    //file lookup in include paths
    let lookup_includs = |name: &dyn AsRef<str>| -> Option<String> {
        for include_path in &args.include_path {
            let path = Path::new(include_path).join(name.as_ref());
            if path.exists() {
                if let Ok(content) = fs::read_to_string(path) {
                    return Some(content);
                }
            }
        }
        None
    };
    //read the glsl
    let f = fs::read_to_string(&args.shader_path)?;
    let shader_source = sdlrig::shaderhelper::include_files(f, lookup_includs);

    //make 10 dummy inputs -_-;
    let data = (0..10)
        .map(|i| {
            let d = VidMixerData::new(VidMixerInfo::from(
                VidMixer::builder()
                    .name(format!("input{i}"))
                    .width(args.width)
                    .height(args.height)
                    .build(),
            ));
            d.prepare(lowlevel_ctx).unwrap();
            d
        })
        .collect::<Vec<_>>();

    let inputs = data
        .iter()
        .map(|d| VidMixerInput::Feedback(&d))
        .collect::<Vec<_>>();
    // make a mixer with the shader
    let m = VidMixerData::new(VidMixerInfo::from(
        VidMixer::builder()
            .name("test".to_string())
            .width(args.width)
            .height(args.height)
            .shader(shader_source)
            .build(),
    ));

    m.mix(
        args.fps,
        1,
        frame,
        &inputs,
        None,
        lowlevel_ctx,
        false,
        true,
        std::ptr::null_mut() as _,
        args.shader_debug,
    )?;
    // window.raise();

    eprintln!("all done.");
    return Ok(());
}
