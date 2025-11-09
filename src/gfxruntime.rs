use crate::gfx_lowlevel::bindings::{
    gfx_lowlevel_destroy_lut, gfx_lowlevel_gpu_ctx, gfx_lowlevel_init_lut, gfx_lowlevel_lut,
};
use crate::gfxinfo::FrameEvent;
use crate::renderspec::{Mix, MixInput, RenderSpec, Reset, SeekVid, SendCmd};
use crate::vidruntime::VidMixerData;
use anyhow::{anyhow, bail, Result};
use ffmpeg_next::Rational;
use sdl2::render::Texture;
use std::ffi::CString;
use std::{cell::RefCell, collections::HashMap};

extern crate ffmpeg_next as ffmpeg;

use crate::{
    gfxinfo::{Asset, GfxInfo},
    texruntime::TexData,
    vidruntime::{VidData, VidInput},
};

#[derive(Debug)]
pub enum GfxData {
    TexData(TexData),
    VidData(VidData),
    VidMixerData(VidMixerData),
}

impl From<TexData> for GfxData {
    fn from(value: TexData) -> Self {
        GfxData::TexData(value)
    }
}

impl From<VidData> for GfxData {
    fn from(value: VidData) -> Self {
        GfxData::VidData(value)
    }
}

impl GfxData {
    pub fn name(&self) -> String {
        match self {
            GfxData::TexData(td) => td.info.name.clone(),
            GfxData::VidData(vd) => vd.info.name.clone(),
            GfxData::VidMixerData(vmd) => vmd.info.name.clone(),
        }
    }

    pub fn info(&self) -> GfxInfo {
        match self {
            GfxData::TexData(td) => td.info.clone().into(),
            GfxData::VidData(vd) => vd.info.clone().into(),
            GfxData::VidMixerData(vmd) => vmd.info().into(),
        }
    }
}

#[derive(Debug)]
pub struct WrapLut(*mut gfx_lowlevel_lut);
unsafe impl Send for WrapLut {}
impl Drop for WrapLut {
    fn drop(&mut self) {
        unsafe {
            gfx_lowlevel_destroy_lut(&mut self.0 as _);
        }
    }
}

pub struct GfxRuntime {
    gfx_info: RefCell<HashMap<String, GfxInfo>>,
    gfx_data: RefCell<HashMap<String, GfxData>>,
    texture: RefCell<HashMap<String, HashMap<usize, Texture>>>,
    stream: RefCell<HashMap<String, VidInput>>,
    pub frames_per_sec: i64,
    pub last_frame_rendered: RefCell<i64>,
    pub lut_cache: RefCell<HashMap<String, WrapLut>>,
}

pub fn load(asset: &Asset) -> Result<GfxData> {
    match asset {
        Asset::Missing => Err(anyhow!("asset is missing")),
        Asset::Tex(t) => TexData::load(t).map(|td| td.into()),
        Asset::Vid(v) => VidData::load(v).map(|vd| vd.into()),
        Asset::VidMixer(m) => Ok(GfxData::VidMixerData(VidMixerData::new(m.clone().into()))),
    }
}

const FFMPEG_INIT_ONCE: std::sync::Once = std::sync::Once::new();

impl GfxRuntime {
    pub fn new(frames_per_sec: i64, frame: i64) -> Self {
        FFMPEG_INIT_ONCE.call_once(|| {
            ffmpeg::init().unwrap();
        });

        Self {
            gfx_info: RefCell::new(HashMap::new()),
            gfx_data: RefCell::new(HashMap::new()),
            texture: RefCell::new(HashMap::new()),
            stream: RefCell::new(HashMap::new()),
            frames_per_sec,
            last_frame_rendered: RefCell::new(frame),
            lut_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn add(&self, add_info: GfxInfo, add_data: GfxData) {
        {
            let info = self.gfx_info.borrow();
            if let Some(current) = info.get(add_info.name()) {
                if current == &add_info {
                    return;
                }
            }
        }

        if let Err(e) = self.remove(add_info.name()) {
            eprintln!("Error removing {}: {}", add_info.name(), e);
        }

        let mut info = self.gfx_info.borrow_mut();
        let mut data = self.gfx_data.borrow_mut();
        let mut texture = self.texture.borrow_mut();
        let tex_map = texture.remove(add_info.name());
        if let Some(mut tex_map) = tex_map {
            for (_, tex) in tex_map.drain() {
                unsafe {
                    tex.destroy();
                }
            }
        }

        let mut stream = self.stream.borrow_mut();
        stream.remove(add_info.name());

        data.insert(add_info.name().clone(), add_data);
        info.insert(add_info.name().clone(), add_info);
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        let mut info = self.gfx_info.borrow_mut();
        let mut data = self.gfx_data.borrow_mut();
        let mut texture = self.texture.borrow_mut();

        info.remove(name);
        data.remove(name);

        if let Some(mut tex_map) = texture.remove(name) {
            for (_, tex) in tex_map.drain() {
                unsafe {
                    tex.destroy();
                }
            }
        }

        Ok(())
    }

    pub fn gfx_info(&self) -> HashMap<String, GfxInfo> {
        self.gfx_info.borrow().clone()
    }

    pub fn set_last_frame_rendered(&self, value: i64) {
        let mut last_frame = self.last_frame_rendered.borrow_mut();
        *last_frame = value;
    }

    pub fn render(
        &self,
        lowlevel_ctx: *mut gfx_lowlevel_gpu_ctx,
        spec: RenderSpec,
        next_frame: i64,
        dry_run: bool,
        shader_debug: bool,
    ) -> Result<()> {
        let last_frame = self.last_frame_rendered.borrow();
        if let Err(e) = match &spec {
            RenderSpec::None => Ok(()),
            RenderSpec::SendCmd(send_cmd) => self.send_cmd(lowlevel_ctx, send_cmd.clone()),
            RenderSpec::HudText(_) => Ok(()),
            RenderSpec::Mix(mix) => self.mix(
                lowlevel_ctx,
                mix,
                next_frame - *last_frame,
                next_frame,
                dry_run,
                shader_debug,
            ),
            RenderSpec::SeekVid(seek_vid) => self.seek_vid(seek_vid, lowlevel_ctx),
            RenderSpec::Reset(reset) => self.reset(reset),
            RenderSpec::SendMidi(_) => Ok(()), // Midi sending is handled elsewhere
        } {
            let msg = format!("Could not render {:?}: {}", spec, e);
            if dry_run {
                panic!("{}", msg);
            } else {
                eprintln!("{}", msg);
            }
        }

        Ok(())
    }

    pub fn mix(
        &self,
        lowlevel_ctx: *mut gfx_lowlevel_gpu_ctx,
        mix: &Mix,
        frames_to_mix: i64,
        frames: i64,
        dry_run: bool,
        shader_debug: bool,
    ) -> Result<()> {
        if lowlevel_ctx.is_null() {
            bail!("Lowlevel context is null");
        }
        if frames_to_mix <= 0 {
            return Ok(());
        }

        let gfx_data = self.gfx_data.borrow();

        let vid_mixer = match gfx_data.get(&mix.name) {
            Some(GfxData::VidMixerData(vid_mixer)) => vid_mixer,
            _ => bail!("No data for mixer data for {}", mix.name),
        };

        let mut inputs = vec![];
        for name in &mix.inputs {
            match name {
                MixInput::Video(name) => {
                    inputs.push(match gfx_data.get(name) {
                        Some(GfxData::VidData(vid_data)) => vid_data.into(),
                        _ => bail!("No such video as {}", name),
                    });
                }
                MixInput::Mixed(name) => inputs.push(match gfx_data.get(name) {
                    Some(GfxData::VidMixerData(vid_mixer_data)) => vid_mixer_data.into(),
                    _ => bail!("No mixer for feedback {}", name),
                }),
            }
        }

        let lut_ptr = if let Some(lut) = mix.lut.as_ref() {
            let mut lut_cache = self.lut_cache.borrow_mut();
            if let Some(cached) = lut_cache.get(&lut.to_string()) {
                cached.0
            } else {
                let lut_str = CString::new(lut.to_string())?;
                unsafe {
                    let lut_ptr = gfx_lowlevel_init_lut(lowlevel_ctx, lut_str.as_ptr());
                    lut_cache.insert(lut.to_string(), WrapLut(lut_ptr));
                    lut_ptr
                }
            }
        } else {
            std::ptr::null_mut()
        };

        match vid_mixer.mix(
            self.frames_per_sec,
            frames_to_mix,
            frames,
            &inputs,
            mix.target.as_ref(),
            lowlevel_ctx,
            dry_run,
            mix.no_display,
            lut_ptr,
            shader_debug,
        ) {
            Err(e) => {
                bail!("Coud not mix frame {:?}: {}", mix.name, e);
            }
            _ => (),
        };

        Ok(())
    }

    fn send_cmd(&self, lowlevel_ctx: *mut gfx_lowlevel_gpu_ctx, send_cmd: SendCmd) -> Result<()> {
        let gfx_data = self.gfx_data.borrow();
        let Some(GfxData::VidMixerData(mix)) = gfx_data.get(&send_cmd.mix) else {
            bail!("No such VidMixer for command {:?}", send_cmd);
        };

        mix.do_cmd(lowlevel_ctx, &send_cmd)?;
        Ok(())
    }

    pub fn get_present_time_for_mix(&self, mix_name: &str) -> Result<Rational> {
        let gfx_data = self.gfx_data.borrow();

        let vid_mixer = match gfx_data.get(mix_name) {
            Some(GfxData::VidMixerData(vid_mixer)) => vid_mixer,
            _ => bail!("No data for mixer data for {}", mix_name),
        };

        vid_mixer.get_present_time()
    }

    fn seek_vid(&self, seek_vid: &SeekVid, lowlevel_ctx: *mut gfx_lowlevel_gpu_ctx) -> Result<()> {
        let gfx_data = self.gfx_data.borrow();
        if let Some(GfxData::VidData(vid_data)) = gfx_data.get(&seek_vid.target) {
            vid_data.seek_vid(seek_vid.sec, seek_vid.exact, lowlevel_ctx)
        } else {
            bail!("No video stream named {}", seek_vid.target)
        }
    }

    pub fn get_last_frame_event(&self, name: &str) -> Result<Option<FrameEvent>> {
        let gfx_data = self.gfx_data.borrow();
        if let Some(GfxData::VidData(vid_data)) = gfx_data.get(name) {
            let last_real_tbu = vid_data.last_real_pts()?.unwrap_or(Rational::new(0, 1));
            let real_ts = last_real_tbu * vid_data.time_base()?;
            let continuous_tbu = Rational::from(vid_data.last_frame_pts()? as f64);
            let continuous_ts = continuous_tbu * vid_data.time_base()?;
            Ok(Some(FrameEvent {
                stream: name.into(),
                real_ts: (real_ts.0, real_ts.1),
                continuous_ts: (continuous_ts.0, continuous_ts.1),
            }))
        } else {
            Ok(None)
        }
    }

    fn reset(&self, reset: &Reset) -> Result<()> {
        let mut gfx_data = self.gfx_data.borrow_mut();
        match gfx_data.get_mut(&reset.target) {
            Some(GfxData::TexData(_tex_data)) => todo!("resetting tex data unimplemented :-o"),
            Some(GfxData::VidData(vid_data)) => vid_data.reset(),
            Some(GfxData::VidMixerData(vid_mixer_data)) => vid_mixer_data.reset(),
            _ => bail!("Unable to find filter named {} to rebuild.", reset.target),
        }
    }
}
