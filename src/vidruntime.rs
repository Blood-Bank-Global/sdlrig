use crate::{
    gfx_lowlevel::bindings::{
        gfx_lowlevel_filter_params, gfx_lowlevel_frame_clear, gfx_lowlevel_frame_create_texture,
        gfx_lowlevel_frame_ctx, gfx_lowlevel_frame_ctx_destroy, gfx_lowlevel_frame_ctx_init,
        gfx_lowlevel_gpu_ctx, gfx_lowlevel_gpu_ctx_render, gfx_lowlevel_lut,
        gfx_lowlevel_map_frame_ctx, gfx_lowlevel_mix_ctx, gfx_lowlevel_mix_ctx_destroy,
        gfx_lowlevel_mix_ctx_init, pl_frame, pl_rect2df, pl_shader_var, pl_var,
        pl_var_type_PL_VAR_FLOAT, pl_var_type_PL_VAR_SINT, pl_var_type_PL_VAR_UINT,
    },
    gfxinfo::{Vid, VidInfo, VidMixerInfo},
    glob::glob,
    renderspec::{CopyEx, SendCmd, SendValue},
};
use anyhow::{bail, Context as AnyhowContext, Error, Result};
use ffmpeg_next::ffi::{AVCodecContext, AVPixelFormat};
use ffmpeg_next::{
    decoder,
    format::{context::Input, input_with_decoder_format},
    frame::Video,
    media::Type,
    Rational,
};

use std::{
    cell::RefCell,
    ffi::{CStr, CString},
    fmt::Debug,
    i32,
    iter::repeat_with,
    sync::Arc,
    usize,
};
extern crate ffmpeg_next as ffmpeg;

#[derive(Debug)]
pub struct VidData {
    pub info: VidInfo,
    pub vid_input: RefCell<Option<VidInput>>,
}

#[derive(Debug)]
pub struct WrapFrame(*mut gfx_lowlevel_frame_ctx);
impl WrapFrame {
    pub fn new(ctx: *mut gfx_lowlevel_gpu_ctx) -> Self {
        unsafe { Self(gfx_lowlevel_frame_ctx_init(ctx)) }
    }
}
unsafe impl Send for WrapFrame {}
unsafe impl Sync for WrapFrame {}
impl Drop for WrapFrame {
    fn drop(&mut self) {
        unsafe {
            gfx_lowlevel_frame_ctx_destroy(&mut self.0 as _);
        }
    }
}

#[derive(Debug)]
pub struct WrapMixCtx(*mut gfx_lowlevel_mix_ctx);
unsafe impl Send for WrapMixCtx {}
impl Drop for WrapMixCtx {
    fn drop(&mut self) {
        unsafe {
            gfx_lowlevel_mix_ctx_destroy(&mut self.0 as _);
        }
    }
}

pub struct VidInput {
    pub ictx: Input,
    pub video_stream_index: usize,
    pub decoder: decoder::Video,
    pub time_base: Rational,
    pub duration_tbu: Rational,
    pub last_frame: Arc<WrapFrame>,
    pub last_frame_pts: i64,
    pub last_frame_duration: i64,
    pub last_real_pts: Option<Rational>,
    pub continuous_pts: Rational,
    pub fps: Rational,
}

impl Debug for VidInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "idx: {:?}, decoder: {:?}, time_base {:?}, duration: {:?}, last_frame.is_empty: {}",
            self.video_stream_index,
            self.decoder.id(),
            self.time_base,
            self.duration_tbu,
            self.last_frame.0 as u64,
        )
    }
}

unsafe extern "C" fn get_hw_format(
    _ctx: *mut AVCodecContext,
    mut pix_fmts: *const AVPixelFormat,
) -> AVPixelFormat {
    while *pix_fmts != AVPixelFormat::AV_PIX_FMT_NONE {
        if *pix_fmts == AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX {
            return *pix_fmts;
        }
        pix_fmts = pix_fmts.offset(1);
    }
    eprintln!("Failed to get HW surface format.\n");
    return AVPixelFormat::AV_PIX_FMT_NONE;
}

impl VidData {
    pub fn load(spec: &Vid) -> Result<VidData> {
        let mut paths = vec![];

        paths.extend(glob(&spec.path).unwrap_or_else(|| {
            vec![spec.path.clone()] // possibly a non glob path
        }));

        if paths.len() == 0 {
            bail!("Nothing loaded for {}", spec.name);
        } else if paths.len() > 1 {
            bail!("Too many files for vid {}, {:?}", spec.name, paths);
        }

        let path = paths[0].clone();
        let ictx = match input_with_decoder_format(
            &path,
            spec.codec.as_ref().map(|s| s.as_str()),
            spec.format.as_ref().map(|s| s.as_str()),
            spec.opts.as_ref().map(|v| v.as_slice()),
        ) {
            Ok(ictx) => ictx,
            Err(e) => bail!(
                "Could not open {} with decoder {:?} and fmt {:?}: {}",
                path,
                spec.codec,
                spec.format,
                e
            ),
        };

        let stream = match ictx
            .streams()
            .best(Type::Video)
            .ok_or(ffmpeg::Error::StreamNotFound)
        {
            Ok(v) => v,
            Err(e) => {
                bail!("Error finding video stream {}", e);
            }
        };

        let context_decoder =
            match ffmpeg::codec::context::Context::from_parameters(stream.parameters()) {
                Ok(context_decoder) => context_decoder,
                Err(e) => bail!("failed to make decoder from input parameters {}", e),
            };

        let decoder = match context_decoder.decoder().video() {
            Ok(v) => v,
            Err(e) => {
                bail!("Error obtaining video decoder {}", e);
            }
        };

        let duration_tbu_q = if stream.duration() > 0 {
            let q = Rational(stream.duration() as i32, 1);
            (q.numerator(), q.denominator())
        } else if ictx.duration() > 0 {
            let q = Rational::new(ictx.duration() as i32, 1);
            (q.numerator(), q.denominator())
        } else {
            (0, 1)
        };

        let timebase_q = {
            let q = stream.time_base();
            (q.numerator(), q.denominator())
        };

        assert!(
            (spec.realtime && !spec.repeat) || !spec.realtime,
            "Cannot be realtime and repeating {:#?}",
            spec
        );

        Ok(VidData {
            info: VidInfo {
                name: spec.name.clone(),
                path,
                repeat: spec.repeat,
                codec: spec.codec.clone(),
                format: spec.format.clone(),
                opts: spec.opts.clone(),
                size: (decoder.width(), decoder.height()),
                duration_tbu_q,
                timebase_q,
                realtime: spec.realtime,
                hardware_decode: spec.hardware_decode,
                software_filter: spec.software_filter,
            },
            vid_input: RefCell::new(None),
        })
    }

    pub fn prepare(&self, lowlevel_ctx: *mut gfx_lowlevel_gpu_ctx) -> Result<()> {
        let mut vid_input = self.vid_input.borrow_mut();

        if vid_input.is_some() {
            return Ok(());
        }

        let path = self.info.path.clone();
        let decoder_name = self.info.codec.as_ref().map(|s| s.as_str());
        let format_name = self.info.format.as_ref().map(|s| s.as_str());
        let ictx = match input_with_decoder_format(
            &path,
            decoder_name,
            format_name,
            self.info.opts.as_ref().map(|v| v.as_slice()),
        ) {
            Ok(ictx) => ictx,
            Err(e) => bail!(
                "Could not preload {} with decoder {:?} and fmt {:?}: {}",
                path,
                decoder_name,
                format_name,
                e
            ),
        };

        let input = ictx
            .streams()
            .best(Type::Video)
            .ok_or(ffmpeg::Error::StreamNotFound)?;

        let video_stream_index = input.index();

        let mut context_decoder = get_codec_context(decoder_name, input.parameters())?;
        if self.info.hardware_decode {
            unsafe {
                let mut hw_device_ctx: *mut ffmpeg_next::ffi::AVBufferRef = std::ptr::null_mut();

                // Set the hw_device_ctx
                if ffmpeg_next::ffi::av_hwdevice_ctx_create(
                    &mut hw_device_ctx as *mut *mut ffmpeg_next::ffi::AVBufferRef,
                    ffmpeg_next::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                ) < 0
                {
                    bail!("Could not create hwdevice context")
                }

                (*context_decoder.as_mut_ptr()).hw_device_ctx =
                    ffmpeg_next::ffi::av_buffer_ref(hw_device_ctx);
                (*context_decoder.as_mut_ptr()).get_format = Some(get_hw_format);
            }
        }
        let decoder = context_decoder.decoder().video()?;

        let Some(stream) = ictx.stream(video_stream_index) else {
            bail!("Could not find video stream");
        };

        let duration: Rational = self.info.duration_tbu_q.into();
        let time_base: Rational = self.info.timebase_q.into();
        let fps = if stream.rate() > Rational::new(0, 1) {
            stream.rate()
        } else if stream.avg_frame_rate() > Rational::new(0, 1) {
            stream.avg_frame_rate()
        } else {
            panic!(
                "Unable to get fps for {} {}",
                self.info.name, self.info.path
            )
        };

        vid_input.replace(VidInput {
            ictx,
            video_stream_index,
            decoder,
            duration_tbu: duration,
            time_base,
            last_frame: Arc::new(WrapFrame::new(lowlevel_ctx)),
            last_frame_pts: 0,
            last_frame_duration: 0,
            last_real_pts: None,
            continuous_pts: Rational::new(0, 1),
            fps,
        });

        Ok(())
    }

    pub fn decode_frame(&self, lowlevel_ctx: *mut gfx_lowlevel_gpu_ctx) -> Result<()> {
        self.prepare(lowlevel_ctx)
            .with_context(|| format!("error preparing {}:{}", file!(), line!()))?;
        let mut borrowed = self.vid_input.borrow_mut();
        let Some(vid_input) = borrowed.as_mut() else {
            bail!("Stream not set after prepre {:?}", self);
        };

        //read from stream
        let mut error_counter = 0;
        loop {
            for (stream, packet) in vid_input.ictx.packets() {
                if stream.index() == vid_input.video_stream_index {
                    vid_input
                        .decoder
                        .send_packet(&packet)
                        .with_context(|| format!("error sending packet {}:{}", file!(), line!()))?;
                    let mut next_decoded = Video::empty();
                    let last_real_pts;
                    match vid_input.decoder.receive_frame(&mut next_decoded) {
                        Ok(()) => {
                            let delta = if self.info.realtime {
                                // go off of timestamps on frames
                                if vid_input.last_frame_duration > 0 {
                                    let delta =
                                        next_decoded.pts().unwrap() - vid_input.last_frame_pts;
                                    unsafe { (*next_decoded.as_mut_ptr()).duration = delta };
                                    last_real_pts = vid_input.continuous_pts;
                                    next_decoded
                                        .set_pts(Some(f64::from(vid_input.continuous_pts) as i64));
                                    Rational::new(delta as i32, 1)
                                } else {
                                    eprintln!("Skip a frame to get a duration....");
                                    vid_input.last_frame_pts = next_decoded.pts().unwrap();
                                    vid_input.last_frame_duration = next_decoded.packet().duration;
                                    continue;
                                }
                            } else {
                                last_real_pts = if let Some(pts) = next_decoded.pts() {
                                    Rational::new(pts as i32, 1)
                                } else {
                                    vid_input.continuous_pts
                                };
                                next_decoded
                                    .set_pts(Some(f64::from(vid_input.continuous_pts) as i64));
                                Rational::new(next_decoded.packet().duration as i32, 1)
                            };
                            vid_input.last_real_pts = Some(last_real_pts);
                            vid_input.continuous_pts = vid_input.continuous_pts + delta;
                            vid_input.last_frame_pts = next_decoded.pts().unwrap();
                            vid_input.last_frame_duration = next_decoded.packet().duration;
                            unsafe {
                                gfx_lowlevel_map_frame_ctx(
                                    lowlevel_ctx,
                                    vid_input.last_frame.0,
                                    next_decoded.as_mut_ptr() as _,
                                );
                            }
                            return Ok(());
                        }
                        Err(ffmpeg_next::Error::Other {
                            errno: ffmpeg_next::ffi::EAGAIN,
                        }) => (), //resource temporarily unavailable
                        Err(e) => {
                            eprintln!("Error receiving frame {}:{}: {}", file!(), line!(), e);
                            error_counter += 1;
                            if error_counter > 2 {
                                return Err(e.into());
                            }
                        }
                    }
                }
            }

            // check to see if we can rewind otherwise break
            if !self.info.repeat || vid_input.duration_tbu <= Rational::new(0, 1) {
                break;
            }

            vid_input
                .ictx
                .seek(0, ..)
                .with_context(|| format!("error seeking {}:{}", file!(), line!()))?;
            vid_input.decoder.flush();
        }

        // We're not looping so just send the last fame forever
        vid_input.continuous_pts =
            vid_input.continuous_pts + Rational::new(vid_input.last_frame_duration as i32, 1);
        vid_input.last_frame_pts = f64::from(vid_input.continuous_pts) as i64;
        // no need to change duration
        return Ok(());
    }

    pub unsafe fn last_frame(&self) -> Result<Option<Arc<WrapFrame>>> {
        let vid_input = self.vid_input.borrow();
        if vid_input.is_none() {
            return Ok(None);
        }
        Ok(Some(vid_input.as_ref().unwrap().last_frame.clone()))
    }

    pub fn last_frame_pts(&self) -> Result<i64> {
        let vid_input = self.vid_input.borrow();
        if vid_input.is_none() {
            return Ok(0);
        }
        Ok(vid_input.as_ref().unwrap().last_frame_pts)
    }

    pub fn last_frame_duration(&self) -> Result<i64> {
        let vid_input = self.vid_input.borrow();
        if vid_input.is_none() {
            return Ok(0);
        }
        Ok(vid_input.as_ref().unwrap().last_frame_duration)
    }

    pub fn last_real_pts(&self) -> Result<Option<Rational>> {
        let vid_input = self.vid_input.borrow();
        if vid_input.is_none() {
            return Ok(None);
        }
        Ok(vid_input.as_ref().unwrap().last_real_pts)
    }

    pub fn time_base(&self) -> Result<Rational> {
        let vid_input = self.vid_input.borrow();
        if vid_input.is_none() {
            return Ok(Rational::new(0, 1));
        }
        Ok(self.vid_input.borrow().as_ref().unwrap().time_base.clone())
    }

    fn duration_tbu(&self) -> Result<Rational> {
        Ok(self
            .vid_input
            .borrow()
            .as_ref()
            .unwrap()
            .duration_tbu
            .clone())
    }

    pub fn seek_vid(
        &self,
        sec: f64,
        exact: bool,
        lowlevel_ctx: *mut gfx_lowlevel_gpu_ctx,
    ) -> Result<()> {
        if self.info.realtime || self.info.repeat == false {
            return Ok(());
        }
        if let Err(e) = self.prepare(lowlevel_ctx) {
            return Err(e);
        }

        let delta_tbu = Rational::from(sec) / self.time_base()?;
        let last_pts = if let Some(last_pts) = self.last_real_pts()? {
            last_pts
        } else {
            Rational::new(0, 1)
        };

        let mut seek_tbu = if exact {
            delta_tbu
        } else {
            last_pts + delta_tbu
        };

        if let Some(stream) = self.vid_input.borrow_mut().as_mut() {
            let mut circuit_breaker = 100;
            while seek_tbu < Rational::new(0, 1) {
                seek_tbu = seek_tbu + stream.duration_tbu;
                circuit_breaker -= 1;
                if circuit_breaker <= 0 {
                    bail!(
                        "Error seeking, too negative {}:{} {seek_tbu}: {sec}",
                        file!(),
                        line!()
                    )
                }
            }
            let mut circuit_breaker = 100;
            while seek_tbu > stream.duration_tbu {
                seek_tbu = seek_tbu - stream.duration_tbu;
                circuit_breaker -= 1;
                if circuit_breaker <= 0 {
                    bail!(
                        "Error seeking, too large {}:{} {seek_tbu}: {sec}",
                        file!(),
                        line!()
                    )
                }
            }
            let ts = f64::from(seek_tbu) as i64;
            if let Err(e) = stream
                .ictx
                .seek_stream(stream.video_stream_index as i32, ts, 0..ts)
            {
                bail!("Error seeking {}:{}: {e}", file!(), line!());
            }
            stream.decoder.flush();
        }

        // We might have hopped to a key frame so let's search for out PTS
        let pts_min = seek_tbu - self.time_base()?.invert() * Rational::new(1, 20);
        let pts_min = if pts_min < Rational::new(0, 0) {
            Rational::new(0, 0)
        } else if pts_min >= self.duration_tbu()? {
            eprintln!("Min somehow beyond duration, just scan the whole thing");
            Rational::new(0, 0)
        } else {
            pts_min
        };
        let mut circuit_breaker = 1000;
        let mut last_last_pts = None;
        loop {
            circuit_breaker -= 1;
            self.decode_frame(lowlevel_ctx)?;
            let last_pts = self.last_real_pts()?.unwrap();
            if last_pts >= pts_min {
                break;
            }
            if let Some(last_last_pts) = last_last_pts {
                if last_pts < last_last_pts {
                    break;
                }
            } else {
                last_last_pts.replace(last_pts);
            }
            if circuit_breaker <= 0 {
                eprintln!(
                    "CIRCUIT BREAKER seek_tbu={seek_tbu:?} min={pts_min:?} duration_tbu={:?}",
                    self.duration_tbu()
                );
                break;
            }
        }
        Ok(())
    }

    pub fn reset(&self) -> Result<()> {
        self.vid_input.borrow_mut().take();
        Ok(())
    }
}

fn get_codec_context(
    name: Option<&str>,
    params: ffmpeg::codec::Parameters,
) -> Result<ffmpeg::codec::Context> {
    let context_decoder = match name {
        Some(decoder_name) => {
            let Some(decoder_codec) = decoder::find_by_name(decoder_name) else {
                bail!("failed to find decoder codec: {}", decoder_name);
            };
            let mut context_decoder =
                ffmpeg::codec::context::Context::new_with_codec(decoder_codec);

            if let Err(e) = context_decoder.set_parameters(params) {
                bail!("Unable to set parameters on codec {}", e);
            }

            context_decoder
        }
        None => match ffmpeg::codec::context::Context::from_parameters(params) {
            Ok(context_decoder) => context_decoder,
            Err(e) => bail!("failed to make decoder from input parameters {}", e),
        },
    };
    Ok(context_decoder)
}

pub struct VidMixerData {
    pub info: VidMixerInfo,
    stream: RefCell<VidMixerStream>,
}

impl Debug for VidMixerData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VidMixerData")
            .field("name", &self.info.name)
            .field("stream", &String::from("[redacted]"))
            .finish()
    }
}

#[derive(Default)]
pub struct VidMixerStream {
    pub next_time: Option<Rational>,
    pub last_input_times: Vec<(VidInfo, Rational)>,
    pub last_frame: Option<Arc<WrapFrame>>,
    pub last_frame_time: Option<Rational>,
    pub frame_count: i64,
    pub mix_ctx: Option<WrapMixCtx>,
    pub has_been_rendered: bool,
}

pub enum VidMixerInput<'a> {
    Video(&'a VidData),
    Feedback(&'a VidMixerData),
}

impl<'a> From<&'a VidData> for VidMixerInput<'a> {
    fn from(value: &'a VidData) -> Self {
        VidMixerInput::Video(value)
    }
}

impl<'a> From<&'a VidMixerData> for VidMixerInput<'a> {
    fn from(value: &'a VidMixerData) -> Self {
        VidMixerInput::Feedback(value)
    }
}

impl VidMixerData {
    pub fn new(info: VidMixerInfo) -> Self {
        Self {
            info,
            stream: RefCell::new(VidMixerStream::default()),
        }
    }

    pub fn info(&self) -> VidMixerInfo {
        self.info.clone()
    }

    fn extract_vars(txt: &str, addendum: &mut String) -> Result<Vec<pl_shader_var>> {
        let mut vars = vec![];
        for line in txt.lines() {
            if line.starts_with("//!VAR ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if (!parts[1].ends_with("[]") && parts.len() < 4) || parts.len() < 3 {
                    continue;
                }
                let name = parts[2].to_string();
                let c_name = CString::new(name.as_bytes()).unwrap();
                let alloc_name = unsafe { libc::malloc(c_name.as_bytes().len() + 1) };
                unsafe {
                    libc::memcpy(
                        alloc_name,
                        c_name.as_ptr() as _,
                        c_name.as_bytes().len() + 1,
                    );
                }

                let (var_type, dim_v, dim_m, dim_a, ptr) = match parts[1] {
                    "float" => {
                        if parts.len() != 4 {
                            eprintln!("Invalid number of parts for float: {}", line);
                            continue;
                        }
                        let data = unsafe { libc::malloc(size_of::<libc::c_float>()) };
                        unsafe {
                            *(data.offset(0) as *mut libc::c_float) =
                                parts[3].parse::<f32>().unwrap_or_default()
                        };
                        (pl_var_type_PL_VAR_FLOAT, 1, 1, 1, data)
                    }
                    "int" => {
                        if parts.len() != 4 {
                            eprintln!("Invalid number of parts for int: {}", line);
                            continue;
                        }
                        let data = unsafe { libc::malloc(size_of::<libc::c_int>()) };
                        unsafe {
                            *(data.offset(0) as *mut libc::c_int) =
                                parts[3].parse::<i32>().unwrap_or_default()
                        };
                        (pl_var_type_PL_VAR_SINT, 1, 1, 1, data)
                    }
                    "uint" => {
                        if parts.len() != 4 {
                            eprintln!("Invalid number of parts for uint: {}", line);
                            continue;
                        }
                        let data = unsafe { libc::malloc(size_of::<libc::c_uint>()) };
                        unsafe {
                            *(data.offset(0) as *mut libc::c_uint) =
                                parts[3].parse::<u32>().unwrap_or_default()
                        };
                        (pl_var_type_PL_VAR_UINT, 1, 1, 1, data)
                    }
                    "vec2" => {
                        if parts.len() != 5 {
                            eprintln!("Invalid number of parts for vec2: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 2;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..2 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 2, 1, 1, data as *mut libc::c_void)
                    }
                    "vec3" => {
                        if parts.len() != 6 {
                            eprintln!("Invalid number of parts for vec3: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 3;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..3 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 3, 1, 1, data as *mut libc::c_void)
                    }
                    "vec4" => {
                        if parts.len() != 7 {
                            eprintln!("Invalid number of parts for vec4: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 4;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..4 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 4, 1, 1, data as *mut libc::c_void)
                    }
                    "ivec2" => {
                        if parts.len() != 5 {
                            eprintln!("Invalid number of parts for ivec2: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_int>() * 2;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_int;
                        for j in 0..2 {
                            let value = parts[3 + j].parse::<i32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_SINT, 2, 1, 1, data as *mut libc::c_void)
                    }
                    "ivec3" => {
                        if parts.len() != 6 {
                            eprintln!("Invalid number of parts for ivec3: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_int>() * 3;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_int;
                        for j in 0..3 {
                            let value = parts[3 + j].parse::<i32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_SINT, 3, 1, 1, data as *mut libc::c_void)
                    }
                    "ivec4" => {
                        if parts.len() != 7 {
                            eprintln!("Invalid number of parts for ivec4: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_int>() * 4;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_int;
                        for j in 0..4 {
                            let value = parts[3 + j].parse::<i32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_SINT, 4, 1, 1, data as *mut libc::c_void)
                    }

                    "uvec2" => {
                        if parts.len() != 5 {
                            eprintln!("Invalid number of parts for uvec2: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_uint>() * 2;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_uint;
                        for j in 0..2 {
                            let value = parts[3 + j].parse::<u32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_UINT, 2, 1, 1, data as *mut libc::c_void)
                    }
                    "uvec3" => {
                        if parts.len() != 6 {
                            eprintln!("Invalid number of parts for uvec3: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_uint>() * 3;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_uint;
                        for j in 0..3 {
                            let value = parts[3 + j].parse::<u32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_UINT, 3, 1, 1, data as *mut libc::c_void)
                    }
                    "uvec4" => {
                        if parts.len() != 7 {
                            eprintln!("Invalid number of parts for uvec4: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_uint>() * 4;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_uint;
                        for j in 0..4 {
                            let value = parts[3 + j].parse::<u32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_UINT, 4, 1, 1, data as *mut libc::c_void)
                    }
                    "mat2x2" | "mat2" => {
                        if parts.len() != 7 {
                            eprintln!("Invalid number of parts for mat2: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 2 * 2;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..4 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 2, 2, 1, data as *mut libc::c_void)
                    }
                    "mat3x3" | "mat3" => {
                        if parts.len() != 11 {
                            eprintln!("Invalid number of parts for mat3: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 3 * 3;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..9 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 3, 3, 1, data as *mut libc::c_void)
                    }
                    "mat4x4" | "mat4" => {
                        if parts.len() != 19 {
                            eprintln!("Invalid number of parts for mat4: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 4 * 4;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..16 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 4, 4, 1, data as *mut libc::c_void)
                    }
                    "mat2x3" => {
                        if parts.len() != 9 {
                            eprintln!("Invalid number of parts for mat2x3: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 2 * 3;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..6 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 2, 3, 1, data as *mut libc::c_void)
                    }
                    "mat2x4" => {
                        if parts.len() != 11 {
                            eprintln!("Invalid number of parts for mat2x4: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 2 * 4;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..8 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 2, 4, 1, data as *mut libc::c_void)
                    }
                    "mat3x2" => {
                        if parts.len() != 9 {
                            eprintln!("Invalid number of parts for mat3x2: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 3 * 2;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..6 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 3, 2, 1, data as *mut libc::c_void)
                    }
                    "mat3x4" => {
                        if parts.len() != 11 {
                            eprintln!("Invalid number of parts for mat3x4: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 3 * 4;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..9 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 3, 4, 1, data as *mut libc::c_void)
                    }
                    "mat4x2" => {
                        if parts.len() != 11 {
                            eprintln!("Invalid number of parts for mat4x2: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 4 * 2;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..8 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 2, 4, 1, data as *mut libc::c_void)
                    }
                    "mat4x3" => {
                        if parts.len() != 15 {
                            eprintln!("Invalid number of parts for mat4x3: {}", line);
                            continue;
                        }
                        let size = size_of::<libc::c_float>() * 4 * 3;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..12 {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        (pl_var_type_PL_VAR_FLOAT, 4, 3, 1, data as *mut libc::c_void)
                    }
                    "int[]" => {
                        if parts.len() < 3 {
                            eprintln!("Invalid number of parts for int[]: {}", line);
                            continue;
                        }

                        let len = (parts.len() - 3).clamp(2, usize::MAX);
                        let size = size_of::<libc::c_int>() * len;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_int;
                        for j in 0..(parts.len() - 3) {
                            let value = parts[3 + j].parse::<i32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        for j in (parts.len() - 3).max(0)..len {
                            unsafe { *(data.offset(j as isize)) = 0 };
                        }
                        (
                            pl_var_type_PL_VAR_SINT,
                            1,
                            1,
                            len,
                            data as *mut libc::c_void,
                        )
                    }
                    "vec2[]" => {
                        if parts.len() < 3 {
                            eprintln!("Invalid number of parts for vec2[]: {}", line);
                            continue;
                        }
                        if parts.len() != 3 && (parts.len() - 3) % 2 != 0 {
                            eprintln!(
                                "Must have an even number of extra values for vec2[]: {}",
                                line
                            );
                            continue;
                        }

                        let len = (parts.len() - 3).clamp(4, usize::MAX);
                        let size = size_of::<libc::c_float>() * len;
                        let data = unsafe { libc::malloc(size) } as *mut libc::c_float;
                        for j in 0..(parts.len() - 3) {
                            let value = parts[3 + j].parse::<f32>().unwrap_or_default();
                            unsafe { *(data.offset(j as isize)) = value };
                        }
                        for j in (parts.len() - 3).max(0)..len {
                            unsafe { *(data.offset(j as isize)) = 0.0 };
                        }
                        (
                            pl_var_type_PL_VAR_FLOAT,
                            2,
                            1,
                            len / 2,
                            data as *mut libc::c_void,
                        )
                    }
                    _ => {
                        eprintln!("Unknown uniform type: {}", line);
                        continue;
                    }
                };

                vars.push(pl_shader_var {
                    var: pl_var {
                        name: alloc_name as _,
                        type_: var_type,
                        dim_v: dim_v as i32,
                        dim_m: dim_m as i32,
                        dim_a: dim_a as i32,
                    },
                    data: ptr as _,
                    dynamic: true,
                });
            } else if line.starts_with("//!STR ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 3 {
                    continue;
                }
                let name = parts[1].to_string();
                let mut chars = vec![];
                let start = line.find("\"");
                let end = line.rfind("\"");
                if let (Some(start), Some(end)) = (start, end) {
                    if start < end {
                        let unescaped =
                            unescaper::unescape(&line[start + 1..end]).unwrap_or(String::new());

                        for j in unescaped.chars() {
                            chars.push(format!("0x{:02x}", j as u32));
                        }

                        if chars.len() >= 128 {
                            chars.truncate(128);
                            addendum
                                .push_str(
                                    &format!("// {} is too long, truncating to 128\n", name,),
                                );
                        }

                        addendum.push_str(&format!("int {}_length = {};\n", name, chars.len()));
                        for _ in chars.len()..128 {
                            chars.push("0x00".to_string());
                        }
                        addendum.push_str(&format!(
                            "int {}[128] = {{{}}};\n",
                            name,
                            chars.join(", ")
                        ));
                    } else {
                        eprintln!("Invalid string declaration (end>=start): {}", line);
                        continue;
                    }
                } else {
                    eprintln!("Invalid string declaration (no quote): {}", line);
                    continue;
                }
            }
        }
        Ok(vars)
    }

    pub fn prepare(&self, lowlevel_ctx: *mut gfx_lowlevel_gpu_ctx) -> Result<()> {
        let mut stream = self.stream.borrow_mut();
        if stream.mix_ctx.is_none() {
            let mut vars = vec![];
            let mut addendum = String::new();

            if let Some(header) = self.info.header.as_ref() {
                vars.extend(Self::extract_vars(header, &mut addendum)?);
            }
            if let Some(prelude) = self.info.prelude.as_ref() {
                vars.extend(Self::extract_vars(prelude, &mut addendum)?);
            }
            if let Some(body) = self.info.body.as_ref() {
                vars.extend(Self::extract_vars(body, &mut addendum)?);
            }

            let prelude_and_addendum = self.info.prelude.as_ref().map_or(addendum.clone(), |p| {
                String::from_iter([p.clone(), addendum.clone()].into_iter())
            });

            let prelude = CString::new(prelude_and_addendum.as_bytes()).unwrap();

            let body = self
                .info
                .body
                .as_ref()
                .map(|s| CString::new(s.as_bytes()).unwrap());

            let header = self
                .info
                .header
                .as_ref()
                .map(|s| CString::new(s.as_bytes()).unwrap());

            //add some internally used variables
            let data = unsafe { libc::malloc(size_of::<libc::c_float>()) };
            let c_name = CString::new("frame".as_bytes()).unwrap();
            let alloc_name = unsafe { libc::malloc(c_name.as_bytes().len() + 1) };
            unsafe {
                libc::memcpy(
                    alloc_name,
                    c_name.as_ptr() as _,
                    c_name.as_bytes().len() + 1,
                );
            }
            vars.push(pl_shader_var {
                var: pl_var {
                    name: alloc_name as _,
                    type_: pl_var_type_PL_VAR_FLOAT,
                    dim_v: 1,
                    dim_m: 1,
                    dim_a: 1,
                },
                data,
                dynamic: false,
            });

            let mix_ctx = unsafe {
                gfx_lowlevel_mix_ctx_init(
                    lowlevel_ctx,
                    prelude.as_ptr(),
                    header.as_ref().map_or(std::ptr::null(), |h| h.as_ptr()),
                    body.as_ref().map_or(std::ptr::null(), |b| b.as_ptr()),
                    vars.as_mut_ptr(),
                    vars.len() as _,
                )
            };
            if mix_ctx.is_null() {
                bail!("Error creating mix ctx for {}", self.info.name);
            }
            stream.mix_ctx.replace(WrapMixCtx(mix_ctx));
            stream.last_frame_time.replace(Rational::new(0, 1));
            if stream.last_frame.is_none() {
                unsafe {
                    stream
                        .last_frame
                        .replace(Arc::new(WrapFrame::new(lowlevel_ctx)));
                    match gfx_lowlevel_frame_create_texture(
                        lowlevel_ctx,
                        stream.last_frame.as_mut().unwrap().0,
                        self.info.width as i32,
                        self.info.height as i32,
                    ) {
                        0 => (),
                        err => bail!("Could not create frame texture {}", err),
                    }
                }
            }
        }
        Ok(())
    }

    pub fn unload(&self) -> Result<()> {
        let mut stream = self.stream.borrow_mut();
        stream.mix_ctx.take();
        Ok(())
    }

    pub fn mix(
        &self,
        fps: i64,
        frames_to_mix: i64,
        frames: i64,
        inputs: &[VidMixerInput],
        target: Option<&CopyEx>,
        lowlevel_ctx: *mut gfx_lowlevel_gpu_ctx,
        dry_run: bool,
        no_display: bool,
        lut_ptr: *mut gfx_lowlevel_lut,
        shader_debug: bool,
    ) -> Result<()> {
        assert!(frames_to_mix > 0);
        self.prepare(lowlevel_ctx)?;
        let mut mix = self.stream.borrow_mut();

        // save the current next time and increment one frame for the object state
        let present_time_secs = mix.next_time.or_else(|| Some(Rational::new(0, 1))).unwrap()
            + Rational::new((frames_to_mix - 1) as i32, fps as i32);
        let one_frame_time_secs = Rational::new(1, fps as i32);

        mix.next_time
            .replace(present_time_secs + one_frame_time_secs);

        if mix.last_input_times.len() != inputs.len() {
            mix.last_input_times.clear();
            mix.last_input_times.extend(
                repeat_with(|| (VidInfo::default(), Rational::new(0, 1))).take(inputs.len()),
            );
        }

        let mut decoded_frames = vec![None; inputs.len()];
        loop {
            for i in 0..inputs.len() {
                match inputs[i] {
                    VidMixerInput::Video(vid_data) => {
                        if vid_data.info.realtime {
                            // just display the next frame you get and forget it
                            // if the input frame rate is slower than the app fps
                            // then you end up stalling and dropping frames '\_(^_^)_/'
                            match vid_data.decode_frame(lowlevel_ctx) {
                                Ok(()) => decoded_frames[i] = unsafe { vid_data.last_frame()? },
                                Err(e) => {
                                    eprintln!(
                                        "Could not decode input {} at {}:{} because {}",
                                        vid_data.info.name,
                                        file!(),
                                        line!(),
                                        e
                                    );
                                    return Err(e);
                                }
                            };
                        } else {
                            let (last_info, last_time) = mix.last_input_times.get_mut(i).unwrap();

                            if last_info != &vid_data.info {
                                *last_info = vid_data.info.clone();
                                *last_time = present_time_secs;
                            }

                            let tbq = Rational::from(vid_data.info.timebase_q);

                            let mut last_frame = unsafe { vid_data.last_frame()? };

                            loop {
                                let last_duration =
                                    Rational::new(vid_data.last_frame_duration()? as i32, 1) * tbq;
                                assert!(last_duration >= Rational::new(0, 1), "negative duration");

                                if last_frame.is_some()
                                    && last_duration != Rational::new(0, 1)
                                    && (*last_time + last_duration) >= present_time_secs
                                {
                                    decoded_frames[i] = last_frame;
                                    break;
                                }

                                *last_time = *last_time + last_duration;
                                // get another frame and update timing
                                last_frame = match vid_data.decode_frame(lowlevel_ctx) {
                                    Ok(()) => unsafe { vid_data.last_frame()? },
                                    Err(e) => {
                                        eprintln!(
                                            "Could not decode input {} at {}:{} because {}",
                                            vid_data.info.name,
                                            file!(),
                                            line!(),
                                            e
                                        );
                                        return Err(e);
                                    }
                                };
                            }
                        }
                    }
                    VidMixerInput::Feedback(vid_mixer_data) => {
                        decoded_frames[i] = if vid_mixer_data.info.name == self.info.name {
                            if !mix.has_been_rendered && mix.last_frame.is_some() {
                                unsafe {
                                    match gfx_lowlevel_frame_clear(
                                        lowlevel_ctx,
                                        &mut (*mix.last_frame.as_mut().unwrap().0).pl_frame as _,
                                        0.0,
                                        0.0,
                                        0.0,
                                        1.0,
                                    ) {
                                        0 => (),
                                        err => bail!("Could not clear frame {}", err),
                                    }
                                    mix.has_been_rendered = true;
                                }
                            }
                            mix.last_frame.clone()
                        } else {
                            let mut other_mix = vid_mixer_data.stream.borrow_mut();
                            if !other_mix.has_been_rendered && other_mix.last_frame.is_some() {
                                unsafe {
                                    match gfx_lowlevel_frame_clear(
                                        lowlevel_ctx,
                                        &mut (*other_mix.last_frame.as_ref().unwrap().0).pl_frame
                                            as _,
                                        0.0,
                                        0.0,
                                        0.0,
                                        1.0,
                                    ) {
                                        0 => (),
                                        err => bail!("Could not clear frame {}", err),
                                    }
                                }
                                other_mix.has_been_rendered = true;
                            }
                            other_mix.last_frame.clone()
                        };
                    }
                }
            }

            break;
        }

        // got a new frame(s)
        mix.last_frame_time = Some(present_time_secs.clone());
        unsafe {
            match gfx_lowlevel_frame_clear(
                lowlevel_ctx,
                &mut (*mix.last_frame.as_mut().unwrap().0).pl_frame as _,
                0.0,
                0.0,
                0.0,
                1.0,
            ) {
                0 => (),
                err => bail!("Could not clear frame {}", err),
            }
        }

        if !decoded_frames.is_empty() && decoded_frames.iter().all(|f| f.is_some()) {
            let mut raw_frames = unsafe {
                decoded_frames
                    .iter()
                    .map(|f| f.as_ref().map(|f| &mut (*f.0).pl_frame as *mut _).unwrap())
                    .collect::<Vec<_>>()
            };

            let num_frames = raw_frames.len() as i32;

            // update how many frames we have seen
            mix.frame_count += 1;

            // update standard vars if requested by the shader

            let target = self.info.name.clone();
            let mut std_vars = vec![
                SendCmd::builder()
                    .name("iFrame")
                    .mix(target.clone())
                    .value(SendValue::Float(mix.frame_count as f32))
                    .build(),
                SendCmd::builder()
                    .name("iResolution")
                    .mix(target.clone())
                    .value(SendValue::Vector(vec![
                        self.info.width as f32,
                        self.info.height as f32,
                        1.0,
                    ]))
                    .build(),
                SendCmd::builder()
                    .name("iTime")
                    .mix(target.clone())
                    .value(SendValue::Float(mix.frame_count as f32 / fps as f32))
                    .build(),
                SendCmd::builder()
                    .name("iTimeDelta")
                    .mix(target.clone())
                    .value(SendValue::Float(f64::from(one_frame_time_secs) as f32))
                    .build(),
                SendCmd::builder()
                    .name("iSampleRate")
                    .mix(target.clone())
                    .value(SendValue::Float(fps as f32))
                    .build(),
            ];

            let mut inp_idx = 0;
            for inp in inputs {
                match inp {
                    &VidMixerInput::Video(vid_data) => std_vars.push(
                        SendCmd::builder()
                            .name(format!("iResolution{inp_idx}"))
                            .value(SendValue::Vector(vec![
                                vid_data.info.size.0 as f32,
                                vid_data.info.size.1 as f32,
                            ]))
                            .mix(target.clone())
                            .build(),
                    ),
                    &VidMixerInput::Feedback(mix_data) => std_vars.push(
                        SendCmd::builder()
                            .name(format!("iResolution{inp_idx}"))
                            .value(SendValue::Vector(vec![
                                mix_data.info.width as f32,
                                mix_data.info.height as f32,
                            ]))
                            .mix(target.clone())
                            .build(),
                    ),
                }
                inp_idx += 1;
            }

            std_vars.push(
                SendCmd::builder()
                    .name("frame")
                    .value(SendValue::Float((frames % (1 << 24)) as f32))
                    .mix(target.clone())
                    .build(),
            );

            let ctx = mix.mix_ctx.as_ref().unwrap().0;
            for c in std_vars {
                self.update_values(ctx, &c)?;
            }

            let params = gfx_lowlevel_filter_params {
                src: pl_rect2df {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 1.0,
                    y1: 1.0,
                },
                dst: pl_rect2df {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 1.0,
                    y1: 1.0,
                },
                rotation: 0.0,
                prelude: unsafe { (*mix.mix_ctx.as_ref().unwrap().0).prelude },
                header: unsafe { (*mix.mix_ctx.as_ref().unwrap().0).header },
                body: unsafe { (*mix.mix_ctx.as_ref().unwrap().0).body },
                vars: unsafe { (*mix.mix_ctx.as_ref().unwrap().0).vars },
                num_vars: unsafe { (*mix.mix_ctx.as_ref().unwrap().0).num_vars },
            };
            unsafe {
                match gfx_lowlevel_gpu_ctx_render(
                    lowlevel_ctx,
                    mix.mix_ctx.as_ref().unwrap().0,
                    &params as _,
                    &mut (*mix.last_frame.as_mut().unwrap().0).pl_frame as _,
                    raw_frames.as_mut_ptr(),
                    num_frames,
                    lut_ptr,
                    shader_debug,
                ) {
                    0 => (),
                    err => bail!("Could not render frame {}", err),
                }
            }
        }

        if dry_run || no_display {
            return Ok(());
        }

        // basic copy params - just sample the mixed frame into the fbo
        let body = CString::new("color = texture(src_tex0, src_coord0);")?;
        let mut params = gfx_lowlevel_filter_params {
            src: pl_rect2df {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            },
            dst: pl_rect2df {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            },
            rotation: 0.0,
            prelude: std::ptr::null() as _,
            header: std::ptr::null() as _,
            body: body.as_ptr(),
            vars: std::ptr::null_mut(),
            num_vars: 0,
        };

        let mut raw_frame =
            unsafe { vec![&mut (*mix.last_frame.as_mut().unwrap().0).pl_frame as *mut pl_frame] };

        if let Some(target) = target.as_ref() {
            if let Some(src) = target.src {
                let w = unsafe { (*(*raw_frame[0]).planes[0].texture).params.w as f32 };
                let h = unsafe { (*(*raw_frame[0]).planes[0].texture).params.h as f32 };
                params.src = pl_rect2df {
                    x0: src.0 as f32 / w,
                    y0: src.1 as f32 / h,
                    x1: (src.0 + src.2 as i32) as f32 / w,
                    y1: (src.1 + src.3 as i32) as f32 / h,
                };
            }
            if let Some(dst) = target.dst {
                let w =
                    unsafe { (*(*lowlevel_ctx).window_frame.planes[0].texture).params.w as f32 };
                let h =
                    unsafe { (*(*lowlevel_ctx).window_frame.planes[0].texture).params.h as f32 };
                params.dst = pl_rect2df {
                    x0: dst.0 as f32 / w,
                    y0: dst.1 as f32 / h,
                    x1: (dst.0 + dst.2 as i32) as f32 / w,
                    y1: (dst.1 + dst.3 as i32) as f32 / h,
                };
            }
        };

        unsafe {
            match gfx_lowlevel_gpu_ctx_render(
                lowlevel_ctx,
                mix.mix_ctx.as_ref().unwrap().0,
                &params as _,
                &mut (*lowlevel_ctx).window_frame as _,
                raw_frame.as_mut_ptr(),
                1,
                std::ptr::null_mut(),
                false,
            ) {
                0 => (),
                err => bail!("Could not render frame {}", err),
            }
        }

        return Ok(());
    }

    pub fn get_present_time(&self) -> Result<Rational> {
        let mix = self.stream.borrow();
        let zero = Rational::new(0, 1);
        return Ok(mix
            .next_time
            .as_ref()
            .or_else(|| Some(&zero))
            .unwrap()
            .clone());
    }

    pub fn reset(&self) -> std::result::Result<(), Error> {
        self.stream.replace(VidMixerStream::default());
        Ok(())
    }

    pub fn do_cmd(
        &self,
        lowlevel_ctx: *mut gfx_lowlevel_gpu_ctx,
        send_cmd: &crate::renderspec::SendCmd,
    ) -> Result<()> {
        self.prepare(lowlevel_ctx)?;
        let stream = self.stream.borrow_mut();
        let ctx = stream.mix_ctx.as_ref().unwrap().0;
        self.update_values(ctx, send_cmd)?;
        Ok(())
    }

    pub fn update_values(
        &self,
        ctx: *mut gfx_lowlevel_mix_ctx,
        send_cmd: &crate::renderspec::SendCmd,
    ) -> Result<()> {
        let name = CString::new(send_cmd.name.as_bytes()).unwrap();
        let num_vars = unsafe { (*ctx).num_vars } as isize;
        for i in 0..num_vars {
            let var = unsafe { (*ctx).vars.offset(i) };
            let var_name = unsafe { CStr::from_ptr((*var).var.name as *mut i8) };
            if var_name == name.as_c_str() {
                match send_cmd.value {
                    SendValue::Float(f) => unsafe {
                        *((*var).data as *mut libc::c_float) = f;
                        return Ok(());
                    },
                    SendValue::Integer(i) => unsafe {
                        *((*var).data as *mut libc::c_int) = i;
                        return Ok(());
                    },
                    SendValue::Unsigned(u) => unsafe {
                        *((*var).data as *mut libc::c_uint) = u;
                        return Ok(());
                    },
                    SendValue::Vector(ref v) => unsafe {
                        let elem_size = ((*var).var.dim_v * (*var).var.dim_m) as usize;
                        let curr_count = (*var).var.dim_a as usize;

                        let len = if curr_count == 1 && elem_size > 1 {
                            v.len().clamp(elem_size, usize::MAX)
                        } else {
                            v.len().clamp(elem_size * 2, usize::MAX)
                        };

                        if len % elem_size != 0 {
                            bail!(
                                "Invalid size for vector {}: expected multiple of {}",
                                send_cmd.name,
                                elem_size
                            );
                        }
                        libc::free((*var).data as *mut libc::c_void);
                        (*var).data =
                            libc::malloc(len * size_of::<libc::c_float>()) as *mut libc::c_void;
                        for j in 0..v.len() {
                            *(((*var).data as *mut libc::c_float).offset(j as isize)) =
                                v[j as usize];
                        }
                        for _ in v.len()..len {
                            *(((*var).data as *mut libc::c_float).offset(v.len() as isize)) = 0.0;
                        }
                        (*var).var.dim_a = (len / elem_size) as i32;
                        return Ok(());
                    },
                    SendValue::IVector(ref v) => unsafe {
                        let elem_size = ((*var).var.dim_v * (*var).var.dim_m) as usize;
                        let curr_count = (*var).var.dim_a as usize;

                        let len = if curr_count == 1 && elem_size > 1 {
                            v.len().clamp(elem_size, usize::MAX)
                        } else {
                            v.len().clamp(elem_size * 2, usize::MAX)
                        };

                        if len % elem_size != 0 {
                            bail!(
                                "Invalid size for vector {}: expected multiple of {}",
                                send_cmd.name,
                                elem_size
                            );
                        }
                        libc::free((*var).data as *mut libc::c_void);
                        (*var).data =
                            libc::malloc(len * size_of::<libc::c_int>()) as *mut libc::c_void;
                        for j in 0..v.len() {
                            *(((*var).data as *mut libc::c_int).offset(j as isize)) = v[j as usize];
                        }
                        for _ in v.len()..len {
                            *(((*var).data as *mut libc::c_int).offset(v.len() as isize)) = 0;
                        }
                        (*var).var.dim_a = (len / elem_size) as i32;
                        return Ok(());
                    },
                    SendValue::UVector(ref v) => unsafe {
                        let elem_size = ((*var).var.dim_v * (*var).var.dim_m) as usize;
                        let curr_count = (*var).var.dim_a as usize;
                        let len = if curr_count == 1 && elem_size > 1 {
                            v.len().clamp(elem_size * 2, usize::MAX)
                        } else {
                            v.len().clamp(elem_size, usize::MAX)
                        };
                        if len % elem_size != 0 {
                            bail!(
                                "Invalid size for vector {}: expected multiple of {}",
                                send_cmd.name,
                                elem_size
                            );
                        }
                        libc::free((*var).data as *mut libc::c_void);
                        (*var).data =
                            libc::malloc(len * size_of::<libc::c_uint>()) as *mut libc::c_void;
                        for j in 0..v.len() {
                            *(((*var).data as *mut libc::c_uint).offset(j as isize)) =
                                v[j as usize];
                        }
                        for _ in v.len()..len {
                            *(((*var).data as *mut libc::c_uint).offset(v.len() as isize)) = 0;
                        }
                        (*var).var.dim_a = (len / elem_size) as i32;
                        return Ok(());
                    },
                }
            }
        }
        Ok(())
    }
}
