use lazy_static::lazy_static;
use sdl2::ttf::{self, Font, Sdl2TtfContext};

lazy_static! {
    static ref FONT_CTX: Sdl2TtfContext = ttf::init().unwrap();
}

pub fn load_font(asset: &str, pt_size: u16) -> Result<Font<'static, 'static>, String> {
    FONT_CTX.load_font(asset, pt_size)
}
