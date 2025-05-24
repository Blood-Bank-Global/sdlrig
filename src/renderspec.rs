use std::{
    error::Error,
    fmt::{Debug, Display},
};

use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[repr(C)]
pub enum RenderSpec {
    #[default]
    None,
    SendCmd(SendCmd),
    HudText(HudText),
    Mix(Mix),
    SeekVid(SeekVid),
    Reset(Reset),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[repr(C)]
pub struct CopyEx {
    pub name: String,
    pub idx: usize,
    pub src: Option<(i32, i32, u32, u32)>,
    pub dst: Option<(i32, i32, u32, u32)>,
    pub rotation: f64,
    pub center: Option<(i32, i32)>,
    pub flip_h: bool,
    pub flip_v: bool,
    pub color_mod: Option<(u8, u8, u8, u8)>,
}

impl CopyEx {
    pub fn builder() -> CopyExBuilder {
        CopyExBuilder::new()
    }
}

pub struct CopyExBuilder {
    obj: CopyEx,
}

impl CopyExBuilder {
    pub fn new() -> Self {
        Self {
            obj: CopyEx::default(),
        }
    }

    pub fn name(mut self, name: String) -> Self {
        self.obj.name = name;
        self
    }

    pub fn idx(mut self, idx: usize) -> Self {
        self.obj.idx = idx;
        self
    }

    pub fn src(mut self, src: (i32, i32, u32, u32)) -> Self {
        self.obj.src = Some(src);
        self
    }

    pub fn dst(mut self, dst: (i32, i32, u32, u32)) -> Self {
        self.obj.dst = Some(dst);
        self
    }

    pub fn rotation(mut self, rotation: f64) -> Self {
        self.obj.rotation = rotation;
        self
    }
    pub fn center(mut self, center: (i32, i32)) -> Self {
        self.obj.center = Some(center);
        self
    }
    pub fn flip_h(mut self, filp_h: bool) -> Self {
        self.obj.flip_h = filp_h;
        self
    }
    pub fn flip_v(mut self, flip_v: bool) -> Self {
        self.obj.flip_v = flip_v;
        self
    }
    pub fn color_mod(mut self, color_mod: (u8, u8, u8, u8)) -> Self {
        self.obj.color_mod = Some(color_mod);
        self
    }

    pub fn build(self) -> CopyEx {
        self.obj.clone()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub enum SendValue {
    Float(f32),
    Integer(i32),
    Unsigned(u32),
    Vector(Vec<f32>), // Also used for matrices
    IVector(Vec<i32>),
    UVector(Vec<u32>),
}

impl Default for SendValue {
    fn default() -> Self {
        SendValue::Float(0.0)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[repr(C)]
pub struct SendCmd {
    pub mix: String,
    pub name: String,
    pub value: SendValue,
}

impl From<SendCmd> for RenderSpec {
    fn from(value: SendCmd) -> Self {
        RenderSpec::SendCmd(value)
    }
}

impl SendCmd {
    pub fn builder() -> SendCmdBuilder {
        SendCmdBuilder::new()
    }
}

pub struct SendCmdBuilder {
    obj: SendCmd,
}

impl SendCmdBuilder {
    pub fn new() -> Self {
        Self {
            obj: SendCmd::default(),
        }
    }

    pub fn mix<T>(mut self, mix: T) -> Self
    where
        T: ToString,
    {
        self.obj.mix = mix.to_string();
        self
    }

    pub fn name<T>(mut self, name: T) -> Self
    where
        T: ToString,
    {
        self.obj.name = name.to_string();
        self
    }

    pub fn value(mut self, value: SendValue) -> Self {
        self.obj.value = value;
        self
    }

    pub fn build(self) -> SendCmd {
        self.obj.clone()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[repr(C)]
pub struct HudText {
    pub text: String,
}

#[macro_export]
macro_rules! hud_text {
    ($text:expr) => {
        sdlrig::renderspec::RenderSpec::HudText(sdlrig::renderspec::HudText {
            text: String::from($text),
        })
    };
}

impl From<HudText> for RenderSpec {
    fn from(value: HudText) -> Self {
        RenderSpec::HudText(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub enum MixInput {
    Video(String),
    Mixed(String),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Mix {
    pub name: String,
    pub inputs: Vec<MixInput>,
    pub target: Option<CopyEx>,
    pub lut: Option<String>,
    pub no_display: bool,
}

impl Mix {
    pub fn builder() -> MixBuilder {
        MixBuilder::new()
    }
}

pub struct MixBuilder {
    obj: Mix,
}

impl MixBuilder {
    pub fn new() -> Self {
        Self {
            obj: Default::default(),
        }
    }

    pub fn build(&self) -> Mix {
        self.obj.clone()
    }

    pub fn name(mut self, name: String) -> Self {
        self.obj.name = name;
        self
    }

    pub fn video(mut self, video: String) -> Self {
        self.obj.inputs.push(MixInput::Video(video));
        self
    }

    pub fn mixed(mut self, mixed: String) -> Self {
        self.obj.inputs.push(MixInput::Mixed(mixed));
        self
    }

    pub fn target(mut self, target: CopyEx) -> Self {
        self.obj.target = Some(target);
        self
    }

    pub fn lut(mut self, lut: String) -> Self {
        self.obj.lut = Some(lut);
        self
    }

    pub fn no_display(mut self, no_display: bool) -> Self {
        self.obj.no_display = no_display;
        self
    }
}

#[macro_export]
macro_rules! mix {
    ( $($k:ident : $v:expr),* ) => {{
        let mut builder = sdlrig::renderspec::MixBuilder::new();
        $(builder = builder.$k($v.into());)*
        sdlrig::renderspec::RenderSpec::Mix(builder.build())
     }};
}

impl From<Mix> for RenderSpec {
    fn from(value: Mix) -> Self {
        RenderSpec::Mix(value)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SeekVid {
    pub target: String,
    pub sec: f64,
    pub exact: bool,
}

#[macro_export]
macro_rules! seek {
    ($target:expr => $sec:expr, $exact:expr) => {
        sdlrig::renderspec::RenderSpec::SeekVid(sdlrig::renderspec::SeekVid {
            target: ($target).into(),
            sec: $sec,
            exact: $exact,
        })
    };
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[repr(C)]
pub struct Reset {
    pub target: String,
}

#[macro_export]
macro_rules! reset {
    ($target:expr) => {
        sdlrig::renderspec::RenderSpec::Reset(sdlrig::renderspec::Reset {
            target: ($target).into(),
        })
    };
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum RenderCalcErr {
    #[default]
    None = 0,
    AssetDataErr,
    Unknown = u8::MAX,
}

impl From<u8> for RenderCalcErr {
    fn from(value: u8) -> Self {
        match value {
            0 => RenderCalcErr::None,
            _ => RenderCalcErr::Unknown,
        }
    }
}

impl Display for RenderCalcErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

impl Error for RenderCalcErr {}
