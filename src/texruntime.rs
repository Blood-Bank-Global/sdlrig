use crate::{
    gfxinfo::{Tex, TexInfo},
    glob::glob,
};
use anyhow::{bail, Result};
use sdl2::{image::ImageRWops, rwops::RWops};
use std::fs;

#[derive(Debug)]
pub struct TexData {
    pub info: TexInfo,
    pub data: Vec<Vec<u8>>,
}

#[cfg(not(target_family = "wasm"))]
impl TexData {
    pub fn load(spec: &Tex) -> Result<TexData> {
        let mut paths = vec![];
        for pattern in &spec.globs {
            paths.extend(glob(&pattern).unwrap_or_else(|| {
                eprintln!("Tex glob returned None ({})", pattern);
                vec![]
            }));
        }

        let mut buffer_list = vec![];
        for path in paths {
            buffer_list.push(match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Tex issue reading bytes from {}, {}", path, e);
                    continue;
                }
            });
        }

        if buffer_list.len() == 0 {
            bail!("Nothing loaded for {}", spec.name);
        }

        let (count, size) = {
            let rw = match RWops::from_bytes(&buffer_list[0]) {
                Ok(rw) => rw,
                Err(e) => {
                    bail!("Could not preload info {}", e);
                }
            };

            let img = match rw.load() {
                Ok(img) => img,
                Err(e) => {
                    bail!("Could not make preload img {}", e);
                }
            };
            (buffer_list.len(), img.size())
        };

        Ok(TexData {
            info: TexInfo {
                name: spec.name.clone(),
                count,
                size,
            },
            data: buffer_list,
        })
    }
}
