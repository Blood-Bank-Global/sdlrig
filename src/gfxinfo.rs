use serde::{Deserialize, Serialize};
use std::i32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GfxInfo {
    TexInfo(TexInfo),
    VidInfo(VidInfo),
    VidMixerInfo(VidMixerInfo),
}

impl GfxInfo {
    pub fn name(&self) -> &String {
        //TODO fix this to be consistent (likely all &str and clone on the caller side)
        match self {
            GfxInfo::TexInfo(v) => &v.name,
            GfxInfo::VidInfo(v) => &v.name,
            GfxInfo::VidMixerInfo(v) => &v.name,
        }
    }
}
macro_rules! gfxinfo_from {
    ($info_type:ident) => {
        impl From<$info_type> for GfxInfo {
            fn from(value: $info_type) -> Self {
                GfxInfo::$info_type(value)
            }
        }
    };
}

gfxinfo_from! { TexInfo }
gfxinfo_from! { VidInfo }
gfxinfo_from! { VidMixerInfo }

//Useful for comparing to assets during loading
impl From<GfxInfo> for Asset {
    fn from(value: GfxInfo) -> Self {
        match value {
            GfxInfo::TexInfo(v) => Asset::Tex(Tex {
                name: v.name,
                globs: vec![],
            }),
            GfxInfo::VidInfo(v) => Asset::Vid(Vid {
                name: v.name,
                path: v.path,
                repeat: v.repeat,
                realtime: v.realtime,
                resolution: v.size,
                tbq: (0, 1),
                codec: v.codec,
                format: v.format,
                opts: v.opts,
                hardware_decode: v.hardware_decode,
                software_filter: v.software_filter,
            }),
            GfxInfo::VidMixerInfo(v) => Asset::VidMixer(VidMixer {
                name: v.name,
                prelude: v.prelude,
                header: v.header,
                body: v.body,
                width: v.width,
                height: v.height,
            }),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq, Hash)]
pub enum Asset {
    #[default]
    Missing,
    Tex(Tex),
    Vid(Vid),
    VidMixer(VidMixer),
}

impl From<Tex> for Asset {
    fn from(value: Tex) -> Self {
        Self::Tex(value)
    }
}

impl From<Vid> for Asset {
    fn from(value: Vid) -> Self {
        Self::Vid(value)
    }
}

impl From<VidMixer> for Asset {
    fn from(value: VidMixer) -> Self {
        Self::VidMixer(value)
    }
}

impl Asset {
    pub fn name(&self) -> &str {
        match self {
            Asset::Missing => "missing",
            Asset::Tex(t) => &t.name,
            Asset::Vid(v) => &v.name,
            Asset::VidMixer(vm) => &vm.name,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TexInfo {
    pub name: String,
    pub count: usize,
    pub size: (u32, u32),
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq, Hash)]
pub struct Tex {
    pub name: String,
    pub globs: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VidInfo {
    pub name: String,
    pub path: String,
    pub repeat: bool,
    pub codec: Option<String>,
    pub format: Option<String>,
    pub opts: Option<Vec<(String, String)>>,
    pub size: (u32, u32),
    pub duration_tbu_q: (i32, i32),
    pub timebase_q: (i32, i32),
    pub realtime: bool,
    pub hardware_decode: bool,
    pub software_filter: bool,
}

impl VidInfo {
    pub fn duration(&self) -> f64 {
        self.duration_tbu_q.0 as f64 / self.duration_tbu_q.1 as f64
    }
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq, Hash)]
pub struct Vid {
    pub name: String,
    pub path: String,
    pub repeat: bool,
    pub realtime: bool,
    pub resolution: (u32, u32),
    pub tbq: (i32, i32),
    pub codec: Option<String>,
    pub format: Option<String>,
    pub opts: Option<Vec<(String, String)>>,
    pub hardware_decode: bool,
    pub software_filter: bool,
}

impl Vid {
    pub fn builder() -> VidBuilder {
        VidBuilder::default()
    }
}

#[derive(Default)]
pub struct VidBuilder {
    pub name: String,
    pub path: String,
    pub repeat: bool,
    pub pix_fmt: String,
    pub resolution: (u32, u32),
    pub tbq: (i32, i32),
    pub codec: Option<String>,
    pub format: Option<String>,
    pub opts: Option<Vec<(String, String)>>,
    pub realtime: bool,
    pub hardware_decode: bool,
    pub software_filter: bool,
}

impl VidBuilder {
    pub fn name<T>(mut self, name: T) -> Self
    where
        T: AsRef<str>,
    {
        self.name = name.as_ref().into();
        self
    }

    pub fn path<T>(mut self, path: T) -> Self
    where
        T: AsRef<str>,
    {
        self.path = path.as_ref().into();
        self
    }

    pub fn repeat(mut self, repeat: bool) -> Self {
        self.repeat = repeat;
        self
    }

    pub fn resolution(mut self, resolution: (u32, u32)) -> Self {
        self.resolution = resolution;
        self
    }

    pub fn pix_fmt(mut self, pix_fmt: &str) -> Self {
        self.pix_fmt = pix_fmt.into();
        self
    }

    pub fn tbq(mut self, tbq: (i32, i32)) -> Self {
        self.tbq = tbq;
        self
    }

    pub fn codec<T>(mut self, codec: T) -> Self
    where
        T: AsRef<str>,
    {
        self.codec = Some(codec.as_ref().into());
        self
    }

    pub fn format<T>(mut self, format: T) -> Self
    where
        T: AsRef<str>,
    {
        self.format = Some(format.as_ref().into());
        self
    }

    pub fn opts<T>(mut self, opts: &[(T, T)]) -> Self
    where
        T: AsRef<str>,
    {
        self.opts = Some(
            opts.iter()
                .map(|(k, v)| (k.as_ref().into(), v.as_ref().into()))
                .collect(),
        );
        self
    }

    pub fn realtime(mut self, realtime: bool) -> Self {
        self.realtime = realtime;
        self
    }

    pub fn hardware_decode(mut self, hardware_decode: bool) -> Self {
        self.hardware_decode = hardware_decode;
        self
    }

    pub fn software_filter(mut self, software_filter: bool) -> Self {
        self.software_filter = software_filter;
        self
    }

    pub fn build(self) -> Vid {
        Vid {
            name: self.name,
            path: self.path,
            repeat: self.repeat,
            resolution: self.resolution,
            tbq: self.tbq,
            codec: self.codec,
            format: self.format,
            opts: self.opts,
            realtime: self.realtime,
            hardware_decode: self.hardware_decode,
            software_filter: self.software_filter,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BufferSrcArgs {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub tbn: i32,
    pub tbd: i32,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq, Hash)]
pub struct VidMixer {
    pub name: String,
    pub prelude: Option<String>,
    pub header: Option<String>,
    pub body: Option<String>,
    pub width: u32,
    pub height: u32,
}

impl VidMixer {
    pub fn builder() -> VidMixerBuilder {
        VidMixerBuilder::new()
    }
}

pub struct VidMixerBuilder {
    name: Option<String>,
    prelude: Option<String>,
    header: Option<String>,
    body: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

impl VidMixerBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            prelude: None,
            header: None,
            body: None,
            width: None,
            height: None,
        }
    }

    pub fn name<T>(mut self, name: T) -> Self
    where
        T: AsRef<str>,
    {
        self.name = Some(name.as_ref().into());
        self
    }

    pub fn prelude<T>(mut self, prelude: T) -> Self
    where
        T: AsRef<str>,
    {
        self.prelude = Some(prelude.as_ref().into());
        self
    }

    pub fn header<T>(mut self, header: T) -> Self
    where
        T: AsRef<str>,
    {
        self.header = Some(header.as_ref().into());
        self
    }

    pub fn body<T>(mut self, body: T) -> Self
    where
        T: AsRef<str>,
    {
        self.body = Some(body.as_ref().into());
        self
    }

    pub fn width(mut self, width: u32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: u32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn build(self) -> VidMixer {
        VidMixer {
            name: self.name.unwrap(),
            prelude: self.prelude,
            header: self.header,
            body: Some(
                self.body
                    .unwrap_or("color = texture(src_tex0, src_coord0);".to_string()),
            ),
            width: self.width.unwrap(),
            height: self.height.unwrap(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq, Hash)]
pub struct VidMixerInfo {
    pub name: String,
    pub prelude: Option<String>,
    pub header: Option<String>,
    pub body: Option<String>,
    pub width: u32,
    pub height: u32,
}

impl From<VidMixer> for VidMixerInfo {
    fn from(value: VidMixer) -> Self {
        Self {
            name: value.name,
            prelude: value.prelude,
            header: value.header,
            body: value.body,
            width: value.width,
            height: value.height,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum Knob {
    B = 0,  // Bottom
    R = 1,  // Top Right
    L = 2,  // Top Left
    CB = 3, // Bottom counter
    CR = 4, // Right counter
    CL = 5, // Left counter
    CF = 6, // Function key counter
}

#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum KeyCode {
    SDLK_UNKNOWN = 0,
    SDLK_RETURN = 13,
    SDLK_ESCAPE = 27,
    SDLK_BACKSPACE = 8,
    SDLK_TAB = 9,
    SDLK_SPACE = 32,
    SDLK_EXCLAIM = 33,
    SDLK_QUOTEDBL = 34,
    SDLK_HASH = 35,
    SDLK_PERCENT = 37,
    SDLK_DOLLAR = 36,
    SDLK_AMPERSAND = 38,
    SDLK_QUOTE = 39,
    SDLK_LEFTPAREN = 40,
    SDLK_RIGHTPAREN = 41,
    SDLK_ASTERISK = 42,
    SDLK_PLUS = 43,
    SDLK_COMMA = 44,
    SDLK_MINUS = 45,
    SDLK_PERIOD = 46,
    SDLK_SLASH = 47,
    SDLK_0 = 48,
    SDLK_1 = 49,
    SDLK_2 = 50,
    SDLK_3 = 51,
    SDLK_4 = 52,
    SDLK_5 = 53,
    SDLK_6 = 54,
    SDLK_7 = 55,
    SDLK_8 = 56,
    SDLK_9 = 57,
    SDLK_COLON = 58,
    SDLK_SEMICOLON = 59,
    SDLK_LESS = 60,
    SDLK_EQUALS = 61,
    SDLK_GREATER = 62,
    SDLK_QUESTION = 63,
    SDLK_AT = 64,
    SDLK_LEFTBRACKET = 91,
    SDLK_BACKSLASH = 92,
    SDLK_RIGHTBRACKET = 93,
    SDLK_CARET = 94,
    SDLK_UNDERSCORE = 95,
    SDLK_BACKQUOTE = 96,
    SDLK_a = 97,
    SDLK_b = 98,
    SDLK_c = 99,
    SDLK_d = 100,
    SDLK_e = 101,
    SDLK_f = 102,
    SDLK_g = 103,
    SDLK_h = 104,
    SDLK_i = 105,
    SDLK_j = 106,
    SDLK_k = 107,
    SDLK_l = 108,
    SDLK_m = 109,
    SDLK_n = 110,
    SDLK_o = 111,
    SDLK_p = 112,
    SDLK_q = 113,
    SDLK_r = 114,
    SDLK_s = 115,
    SDLK_t = 116,
    SDLK_u = 117,
    SDLK_v = 118,
    SDLK_w = 119,
    SDLK_x = 120,
    SDLK_y = 121,
    SDLK_z = 122,
    SDLK_CAPSLOCK = 1073741881,
    SDLK_F1 = 1073741882,
    SDLK_F2 = 1073741883,
    SDLK_F3 = 1073741884,
    SDLK_F4 = 1073741885,
    SDLK_F5 = 1073741886,
    SDLK_F6 = 1073741887,
    SDLK_F7 = 1073741888,
    SDLK_F8 = 1073741889,
    SDLK_F9 = 1073741890,
    SDLK_F10 = 1073741891,
    SDLK_F11 = 1073741892,
    SDLK_F12 = 1073741893,
    SDLK_PRINTSCREEN = 1073741894,
    SDLK_SCROLLLOCK = 1073741895,
    SDLK_PAUSE = 1073741896,
    SDLK_INSERT = 1073741897,
    SDLK_HOME = 1073741898,
    SDLK_PAGEUP = 1073741899,
    SDLK_DELETE = 127,
    SDLK_END = 1073741901,
    SDLK_PAGEDOWN = 1073741902,
    SDLK_RIGHT = 1073741903,
    SDLK_LEFT = 1073741904,
    SDLK_DOWN = 1073741905,
    SDLK_UP = 1073741906,
    SDLK_NUMLOCKCLEAR = 1073741907,
    SDLK_KP_DIVIDE = 1073741908,
    SDLK_KP_MULTIPLY = 1073741909,
    SDLK_KP_MINUS = 1073741910,
    SDLK_KP_PLUS = 1073741911,
    SDLK_KP_ENTER = 1073741912,
    SDLK_KP_1 = 1073741913,
    SDLK_KP_2 = 1073741914,
    SDLK_KP_3 = 1073741915,
    SDLK_KP_4 = 1073741916,
    SDLK_KP_5 = 1073741917,
    SDLK_KP_6 = 1073741918,
    SDLK_KP_7 = 1073741919,
    SDLK_KP_8 = 1073741920,
    SDLK_KP_9 = 1073741921,
    SDLK_KP_0 = 1073741922,
    SDLK_KP_PERIOD = 1073741923,
    SDLK_APPLICATION = 1073741925,
    SDLK_POWER = 1073741926,
    SDLK_KP_EQUALS = 1073741927,
    SDLK_F13 = 1073741928,
    SDLK_F14 = 1073741929,
    SDLK_F15 = 1073741930,
    SDLK_F16 = 1073741931,
    SDLK_F17 = 1073741932,
    SDLK_F18 = 1073741933,
    SDLK_F19 = 1073741934,
    SDLK_F20 = 1073741935,
    SDLK_F21 = 1073741936,
    SDLK_F22 = 1073741937,
    SDLK_F23 = 1073741938,
    SDLK_F24 = 1073741939,
    SDLK_EXECUTE = 1073741940,
    SDLK_HELP = 1073741941,
    SDLK_MENU = 1073741942,
    SDLK_SELECT = 1073741943,
    SDLK_STOP = 1073741944,
    SDLK_AGAIN = 1073741945,
    SDLK_UNDO = 1073741946,
    SDLK_CUT = 1073741947,
    SDLK_COPY = 1073741948,
    SDLK_PASTE = 1073741949,
    SDLK_FIND = 1073741950,
    SDLK_MUTE = 1073741951,
    SDLK_VOLUMEUP = 1073741952,
    SDLK_VOLUMEDOWN = 1073741953,
    SDLK_KP_COMMA = 1073741957,
    SDLK_KP_EQUALSAS400 = 1073741958,
    SDLK_ALTERASE = 1073741977,
    SDLK_SYSREQ = 1073741978,
    SDLK_CANCEL = 1073741979,
    SDLK_CLEAR = 1073741980,
    SDLK_PRIOR = 1073741981,
    SDLK_RETURN2 = 1073741982,
    SDLK_SEPARATOR = 1073741983,
    SDLK_OUT = 1073741984,
    SDLK_OPER = 1073741985,
    SDLK_CLEARAGAIN = 1073741986,
    SDLK_CRSEL = 1073741987,
    SDLK_EXSEL = 1073741988,
    SDLK_KP_00 = 1073742000,
    SDLK_KP_000 = 1073742001,
    SDLK_THOUSANDSSEPARATOR = 1073742002,
    SDLK_DECIMALSEPARATOR = 1073742003,
    SDLK_CURRENCYUNIT = 1073742004,
    SDLK_CURRENCYSUBUNIT = 1073742005,
    SDLK_KP_LEFTPAREN = 1073742006,
    SDLK_KP_RIGHTPAREN = 1073742007,
    SDLK_KP_LEFTBRACE = 1073742008,
    SDLK_KP_RIGHTBRACE = 1073742009,
    SDLK_KP_TAB = 1073742010,
    SDLK_KP_BACKSPACE = 1073742011,
    SDLK_KP_A = 1073742012,
    SDLK_KP_B = 1073742013,
    SDLK_KP_C = 1073742014,
    SDLK_KP_D = 1073742015,
    SDLK_KP_E = 1073742016,
    SDLK_KP_F = 1073742017,
    SDLK_KP_XOR = 1073742018,
    SDLK_KP_POWER = 1073742019,
    SDLK_KP_PERCENT = 1073742020,
    SDLK_KP_LESS = 1073742021,
    SDLK_KP_GREATER = 1073742022,
    SDLK_KP_AMPERSAND = 1073742023,
    SDLK_KP_DBLAMPERSAND = 1073742024,
    SDLK_KP_VERTICALBAR = 1073742025,
    SDLK_KP_DBLVERTICALBAR = 1073742026,
    SDLK_KP_COLON = 1073742027,
    SDLK_KP_HASH = 1073742028,
    SDLK_KP_SPACE = 1073742029,
    SDLK_KP_AT = 1073742030,
    SDLK_KP_EXCLAM = 1073742031,
    SDLK_KP_MEMSTORE = 1073742032,
    SDLK_KP_MEMRECALL = 1073742033,
    SDLK_KP_MEMCLEAR = 1073742034,
    SDLK_KP_MEMADD = 1073742035,
    SDLK_KP_MEMSUBTRACT = 1073742036,
    SDLK_KP_MEMMULTIPLY = 1073742037,
    SDLK_KP_MEMDIVIDE = 1073742038,
    SDLK_KP_PLUSMINUS = 1073742039,
    SDLK_KP_CLEAR = 1073742040,
    SDLK_KP_CLEARENTRY = 1073742041,
    SDLK_KP_BINARY = 1073742042,
    SDLK_KP_OCTAL = 1073742043,
    SDLK_KP_DECIMAL = 1073742044,
    SDLK_KP_HEXADECIMAL = 1073742045,
    SDLK_LCTRL = 1073742048,
    SDLK_LSHIFT = 1073742049,
    SDLK_LALT = 1073742050,
    SDLK_LGUI = 1073742051,
    SDLK_RCTRL = 1073742052,
    SDLK_RSHIFT = 1073742053,
    SDLK_RALT = 1073742054,
    SDLK_RGUI = 1073742055,
    SDLK_MODE = 1073742081,
    SDLK_AUDIONEXT = 1073742082,
    SDLK_AUDIOPREV = 1073742083,
    SDLK_AUDIOSTOP = 1073742084,
    SDLK_AUDIOPLAY = 1073742085,
    SDLK_AUDIOMUTE = 1073742086,
    SDLK_MEDIASELECT = 1073742087,
    SDLK_WWW = 1073742088,
    SDLK_MAIL = 1073742089,
    SDLK_CALCULATOR = 1073742090,
    SDLK_COMPUTER = 1073742091,
    SDLK_AC_SEARCH = 1073742092,
    SDLK_AC_HOME = 1073742093,
    SDLK_AC_BACK = 1073742094,
    SDLK_AC_FORWARD = 1073742095,
    SDLK_AC_STOP = 1073742096,
    SDLK_AC_REFRESH = 1073742097,
    SDLK_AC_BOOKMARKS = 1073742098,
    SDLK_BRIGHTNESSDOWN = 1073742099,
    SDLK_BRIGHTNESSUP = 1073742100,
    SDLK_DISPLAYSWITCH = 1073742101,
    SDLK_KBDILLUMTOGGLE = 1073742102,
    SDLK_KBDILLUMDOWN = 1073742103,
    SDLK_KBDILLUMUP = 1073742104,
    SDLK_EJECT = 1073742105,
    SDLK_SLEEP = 1073742106,
    SDLK_APP1 = 1073742107,
    SDLK_APP2 = 1073742108,
    SDLK_AUDIOREWIND = 1073742109,
    SDLK_AUDIOFASTFORWARD = 1073742110,
}

impl From<u32> for KeyCode {
    fn from(value: u32) -> Self {
        match value {
            0 => KeyCode::SDLK_UNKNOWN,
            13 => KeyCode::SDLK_RETURN,
            27 => KeyCode::SDLK_ESCAPE,
            8 => KeyCode::SDLK_BACKSPACE,
            9 => KeyCode::SDLK_TAB,
            32 => KeyCode::SDLK_SPACE,
            33 => KeyCode::SDLK_EXCLAIM,
            34 => KeyCode::SDLK_QUOTEDBL,
            35 => KeyCode::SDLK_HASH,
            37 => KeyCode::SDLK_PERCENT,
            36 => KeyCode::SDLK_DOLLAR,
            38 => KeyCode::SDLK_AMPERSAND,
            39 => KeyCode::SDLK_QUOTE,
            40 => KeyCode::SDLK_LEFTPAREN,
            41 => KeyCode::SDLK_RIGHTPAREN,
            42 => KeyCode::SDLK_ASTERISK,
            43 => KeyCode::SDLK_PLUS,
            44 => KeyCode::SDLK_COMMA,
            45 => KeyCode::SDLK_MINUS,
            46 => KeyCode::SDLK_PERIOD,
            47 => KeyCode::SDLK_SLASH,
            48 => KeyCode::SDLK_0,
            49 => KeyCode::SDLK_1,
            50 => KeyCode::SDLK_2,
            51 => KeyCode::SDLK_3,
            52 => KeyCode::SDLK_4,
            53 => KeyCode::SDLK_5,
            54 => KeyCode::SDLK_6,
            55 => KeyCode::SDLK_7,
            56 => KeyCode::SDLK_8,
            57 => KeyCode::SDLK_9,
            58 => KeyCode::SDLK_COLON,
            59 => KeyCode::SDLK_SEMICOLON,
            60 => KeyCode::SDLK_LESS,
            61 => KeyCode::SDLK_EQUALS,
            62 => KeyCode::SDLK_GREATER,
            63 => KeyCode::SDLK_QUESTION,
            64 => KeyCode::SDLK_AT,
            91 => KeyCode::SDLK_LEFTBRACKET,
            92 => KeyCode::SDLK_BACKSLASH,
            93 => KeyCode::SDLK_RIGHTBRACKET,
            94 => KeyCode::SDLK_CARET,
            95 => KeyCode::SDLK_UNDERSCORE,
            96 => KeyCode::SDLK_BACKQUOTE,
            97 => KeyCode::SDLK_a,
            98 => KeyCode::SDLK_b,
            99 => KeyCode::SDLK_c,
            100 => KeyCode::SDLK_d,
            101 => KeyCode::SDLK_e,
            102 => KeyCode::SDLK_f,
            103 => KeyCode::SDLK_g,
            104 => KeyCode::SDLK_h,
            105 => KeyCode::SDLK_i,
            106 => KeyCode::SDLK_j,
            107 => KeyCode::SDLK_k,
            108 => KeyCode::SDLK_l,
            109 => KeyCode::SDLK_m,
            110 => KeyCode::SDLK_n,
            111 => KeyCode::SDLK_o,
            112 => KeyCode::SDLK_p,
            113 => KeyCode::SDLK_q,
            114 => KeyCode::SDLK_r,
            115 => KeyCode::SDLK_s,
            116 => KeyCode::SDLK_t,
            117 => KeyCode::SDLK_u,
            118 => KeyCode::SDLK_v,
            119 => KeyCode::SDLK_w,
            120 => KeyCode::SDLK_x,
            121 => KeyCode::SDLK_y,
            122 => KeyCode::SDLK_z,
            1073741881 => KeyCode::SDLK_CAPSLOCK,
            1073741882 => KeyCode::SDLK_F1,
            1073741883 => KeyCode::SDLK_F2,
            1073741884 => KeyCode::SDLK_F3,
            1073741885 => KeyCode::SDLK_F4,
            1073741886 => KeyCode::SDLK_F5,
            1073741887 => KeyCode::SDLK_F6,
            1073741888 => KeyCode::SDLK_F7,
            1073741889 => KeyCode::SDLK_F8,
            1073741890 => KeyCode::SDLK_F9,
            1073741891 => KeyCode::SDLK_F10,
            1073741892 => KeyCode::SDLK_F11,
            1073741893 => KeyCode::SDLK_F12,
            1073741894 => KeyCode::SDLK_PRINTSCREEN,
            1073741895 => KeyCode::SDLK_SCROLLLOCK,
            1073741896 => KeyCode::SDLK_PAUSE,
            1073741897 => KeyCode::SDLK_INSERT,
            1073741898 => KeyCode::SDLK_HOME,
            1073741899 => KeyCode::SDLK_PAGEUP,
            127 => KeyCode::SDLK_DELETE,
            1073741901 => KeyCode::SDLK_END,
            1073741902 => KeyCode::SDLK_PAGEDOWN,
            1073741903 => KeyCode::SDLK_RIGHT,
            1073741904 => KeyCode::SDLK_LEFT,
            1073741905 => KeyCode::SDLK_DOWN,
            1073741906 => KeyCode::SDLK_UP,
            1073741907 => KeyCode::SDLK_NUMLOCKCLEAR,
            1073741908 => KeyCode::SDLK_KP_DIVIDE,
            1073741909 => KeyCode::SDLK_KP_MULTIPLY,
            1073741910 => KeyCode::SDLK_KP_MINUS,
            1073741911 => KeyCode::SDLK_KP_PLUS,
            1073741912 => KeyCode::SDLK_KP_ENTER,
            1073741913 => KeyCode::SDLK_KP_1,
            1073741914 => KeyCode::SDLK_KP_2,
            1073741915 => KeyCode::SDLK_KP_3,
            1073741916 => KeyCode::SDLK_KP_4,
            1073741917 => KeyCode::SDLK_KP_5,
            1073741918 => KeyCode::SDLK_KP_6,
            1073741919 => KeyCode::SDLK_KP_7,
            1073741920 => KeyCode::SDLK_KP_8,
            1073741921 => KeyCode::SDLK_KP_9,
            1073741922 => KeyCode::SDLK_KP_0,
            1073741923 => KeyCode::SDLK_KP_PERIOD,
            1073741925 => KeyCode::SDLK_APPLICATION,
            1073741926 => KeyCode::SDLK_POWER,
            1073741927 => KeyCode::SDLK_KP_EQUALS,
            1073741928 => KeyCode::SDLK_F13,
            1073741929 => KeyCode::SDLK_F14,
            1073741930 => KeyCode::SDLK_F15,
            1073741931 => KeyCode::SDLK_F16,
            1073741932 => KeyCode::SDLK_F17,
            1073741933 => KeyCode::SDLK_F18,
            1073741934 => KeyCode::SDLK_F19,
            1073741935 => KeyCode::SDLK_F20,
            1073741936 => KeyCode::SDLK_F21,
            1073741937 => KeyCode::SDLK_F22,
            1073741938 => KeyCode::SDLK_F23,
            1073741939 => KeyCode::SDLK_F24,
            1073741940 => KeyCode::SDLK_EXECUTE,
            1073741941 => KeyCode::SDLK_HELP,
            1073741942 => KeyCode::SDLK_MENU,
            1073741943 => KeyCode::SDLK_SELECT,
            1073741944 => KeyCode::SDLK_STOP,
            1073741945 => KeyCode::SDLK_AGAIN,
            1073741946 => KeyCode::SDLK_UNDO,
            1073741947 => KeyCode::SDLK_CUT,
            1073741948 => KeyCode::SDLK_COPY,
            1073741949 => KeyCode::SDLK_PASTE,
            1073741950 => KeyCode::SDLK_FIND,
            1073741951 => KeyCode::SDLK_MUTE,
            1073741952 => KeyCode::SDLK_VOLUMEUP,
            1073741953 => KeyCode::SDLK_VOLUMEDOWN,
            1073741957 => KeyCode::SDLK_KP_COMMA,
            1073741958 => KeyCode::SDLK_KP_EQUALSAS400,
            1073741977 => KeyCode::SDLK_ALTERASE,
            1073741978 => KeyCode::SDLK_SYSREQ,
            1073741979 => KeyCode::SDLK_CANCEL,
            1073741980 => KeyCode::SDLK_CLEAR,
            1073741981 => KeyCode::SDLK_PRIOR,
            1073741982 => KeyCode::SDLK_RETURN2,
            1073741983 => KeyCode::SDLK_SEPARATOR,
            1073741984 => KeyCode::SDLK_OUT,
            1073741985 => KeyCode::SDLK_OPER,
            1073741986 => KeyCode::SDLK_CLEARAGAIN,
            1073741987 => KeyCode::SDLK_CRSEL,
            1073741988 => KeyCode::SDLK_EXSEL,
            1073742000 => KeyCode::SDLK_KP_00,
            1073742001 => KeyCode::SDLK_KP_000,
            1073742002 => KeyCode::SDLK_THOUSANDSSEPARATOR,
            1073742003 => KeyCode::SDLK_DECIMALSEPARATOR,
            1073742004 => KeyCode::SDLK_CURRENCYUNIT,
            1073742005 => KeyCode::SDLK_CURRENCYSUBUNIT,
            1073742006 => KeyCode::SDLK_KP_LEFTPAREN,
            1073742007 => KeyCode::SDLK_KP_RIGHTPAREN,
            1073742008 => KeyCode::SDLK_KP_LEFTBRACE,
            1073742009 => KeyCode::SDLK_KP_RIGHTBRACE,
            1073742010 => KeyCode::SDLK_KP_TAB,
            1073742011 => KeyCode::SDLK_KP_BACKSPACE,
            1073742012 => KeyCode::SDLK_KP_A,
            1073742013 => KeyCode::SDLK_KP_B,
            1073742014 => KeyCode::SDLK_KP_C,
            1073742015 => KeyCode::SDLK_KP_D,
            1073742016 => KeyCode::SDLK_KP_E,
            1073742017 => KeyCode::SDLK_KP_F,
            1073742018 => KeyCode::SDLK_KP_XOR,
            1073742019 => KeyCode::SDLK_KP_POWER,
            1073742020 => KeyCode::SDLK_KP_PERCENT,
            1073742021 => KeyCode::SDLK_KP_LESS,
            1073742022 => KeyCode::SDLK_KP_GREATER,
            1073742023 => KeyCode::SDLK_KP_AMPERSAND,
            1073742024 => KeyCode::SDLK_KP_DBLAMPERSAND,
            1073742025 => KeyCode::SDLK_KP_VERTICALBAR,
            1073742026 => KeyCode::SDLK_KP_DBLVERTICALBAR,
            1073742027 => KeyCode::SDLK_KP_COLON,
            1073742028 => KeyCode::SDLK_KP_HASH,
            1073742029 => KeyCode::SDLK_KP_SPACE,
            1073742030 => KeyCode::SDLK_KP_AT,
            1073742031 => KeyCode::SDLK_KP_EXCLAM,
            1073742032 => KeyCode::SDLK_KP_MEMSTORE,
            1073742033 => KeyCode::SDLK_KP_MEMRECALL,
            1073742034 => KeyCode::SDLK_KP_MEMCLEAR,
            1073742035 => KeyCode::SDLK_KP_MEMADD,
            1073742036 => KeyCode::SDLK_KP_MEMSUBTRACT,
            1073742037 => KeyCode::SDLK_KP_MEMMULTIPLY,
            1073742038 => KeyCode::SDLK_KP_MEMDIVIDE,
            1073742039 => KeyCode::SDLK_KP_PLUSMINUS,
            1073742040 => KeyCode::SDLK_KP_CLEAR,
            1073742041 => KeyCode::SDLK_KP_CLEARENTRY,
            1073742042 => KeyCode::SDLK_KP_BINARY,
            1073742043 => KeyCode::SDLK_KP_OCTAL,
            1073742044 => KeyCode::SDLK_KP_DECIMAL,
            1073742045 => KeyCode::SDLK_KP_HEXADECIMAL,
            1073742048 => KeyCode::SDLK_LCTRL,
            1073742049 => KeyCode::SDLK_LSHIFT,
            1073742050 => KeyCode::SDLK_LALT,
            1073742051 => KeyCode::SDLK_LGUI,
            1073742052 => KeyCode::SDLK_RCTRL,
            1073742053 => KeyCode::SDLK_RSHIFT,
            1073742054 => KeyCode::SDLK_RALT,
            1073742055 => KeyCode::SDLK_RGUI,
            1073742081 => KeyCode::SDLK_MODE,
            1073742082 => KeyCode::SDLK_AUDIONEXT,
            1073742083 => KeyCode::SDLK_AUDIOPREV,
            1073742084 => KeyCode::SDLK_AUDIOSTOP,
            1073742085 => KeyCode::SDLK_AUDIOPLAY,
            1073742086 => KeyCode::SDLK_AUDIOMUTE,
            1073742087 => KeyCode::SDLK_MEDIASELECT,
            1073742088 => KeyCode::SDLK_WWW,
            1073742089 => KeyCode::SDLK_MAIL,
            1073742090 => KeyCode::SDLK_CALCULATOR,
            1073742091 => KeyCode::SDLK_COMPUTER,
            1073742092 => KeyCode::SDLK_AC_SEARCH,
            1073742093 => KeyCode::SDLK_AC_HOME,
            1073742094 => KeyCode::SDLK_AC_BACK,
            1073742095 => KeyCode::SDLK_AC_FORWARD,
            1073742096 => KeyCode::SDLK_AC_STOP,
            1073742097 => KeyCode::SDLK_AC_REFRESH,
            1073742098 => KeyCode::SDLK_AC_BOOKMARKS,
            1073742099 => KeyCode::SDLK_BRIGHTNESSDOWN,
            1073742100 => KeyCode::SDLK_BRIGHTNESSUP,
            1073742101 => KeyCode::SDLK_DISPLAYSWITCH,
            1073742102 => KeyCode::SDLK_KBDILLUMTOGGLE,
            1073742103 => KeyCode::SDLK_KBDILLUMDOWN,
            1073742104 => KeyCode::SDLK_KBDILLUMUP,
            1073742105 => KeyCode::SDLK_EJECT,
            1073742106 => KeyCode::SDLK_SLEEP,
            1073742107 => KeyCode::SDLK_APP1,
            1073742108 => KeyCode::SDLK_APP2,
            1073742109 => KeyCode::SDLK_AUDIOREWIND,
            1073742110 => KeyCode::SDLK_AUDIOFASTFORWARD,
            _ => KeyCode::SDLK_UNKNOWN,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyEvent {
    pub key: KeyCode,
    pub shift: bool,
    pub alt: bool,
    pub ctl: bool,
    pub down: bool,
    pub repeat: bool,
    pub timestamp: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameEvent {
    pub stream: String,
    pub real_ts: (i32, i32),
    pub continuous_ts: (i32, i32),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum GfxEvent {
    KeyEvent(KeyEvent),
    FrameEvent(FrameEvent),
    ReloadEvent(),
}
