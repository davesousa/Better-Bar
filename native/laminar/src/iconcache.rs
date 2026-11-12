//! On-disk icon cache: `%LOCALAPPDATA%\Laminar\icons\`.
//!
//! Asking the Windows shell for an icon is slow (tens of ms each) and pulls a lot
//! of shell code into memory. The first time Laminar sees an app it asks the
//! shell once and saves a 256 px PNG plus the display name; afterwards it only
//! decodes the PNG.

use std::collections::HashMap;
use std::path::PathBuf;

use windows::core::{Result, HSTRING};
use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
use windows::Win32::Graphics::Gdi::{DeleteObject, HPALETTE};
use windows::Win32::Graphics::Imaging::*;

use crate::apps;

pub struct IconCache {
    dir: PathBuf,
    /// app id (lower case) → display name
    names: HashMap<String, String>,
    /// pinned app id → the program it launches (lower case), "" if unknown
    targets: HashMap<String, String>,
    dirty: bool,
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.to_lowercase().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

impl IconCache {
    pub fn open() -> Self {
        let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let dir = base.join("Laminar").join("icons");
        let _ = std::fs::create_dir_all(&dir);
        let names = std::fs::read_to_string(dir.join("names.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let targets = std::fs::read_to_string(dir.join("targets.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self { dir, names, targets, dirty: false }
    }

    /// The program a pinned app id launches, used to match running windows to pins.
    /// Asking the shell is slow (~20 ms each), so answers are cached on disk.
    pub fn target(&mut self, id: &str) -> Option<String> {
        let key = id.to_lowercase();
        if let Some(t) = self.targets.get(&key) {
            return if t.is_empty() { None } else { Some(t.clone()) };
        }
        let t = apps::resolve_path(id);
        self.targets.insert(key, t.clone().unwrap_or_default());
        self.dirty = true;
        t
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{:016x}.png", fnv(id)))
    }

    /// Decode a cached icon without ever asking the shell (fast; for Launchpad tiles).
    pub fn get_cached(&self, wic: &IWICImagingFactory, id: &str) -> Option<IWICBitmapSource> {
        let file = self.file(id);
        if file.exists() {
            decode(wic, &file).ok()
        } else {
            None
        }
    }

    /// Record a display name learned elsewhere (e.g. the background icon worker).
    pub fn set_name(&mut self, id: &str, name: &str) {
        self.names.insert(id.to_lowercase(), name.to_string());
        self.dirty = true;
    }

    pub fn dir(&self) -> PathBuf {
        self.dir.clone()
    }

    /// Icon (as premultiplied BGRA, ready for Direct2D) and display name for an app id.
    pub fn get(&mut self, wic: &IWICImagingFactory, id: &str) -> Option<(String, IWICBitmapSource)> {
        let key = id.to_lowercase();
        let file = self.file(id);
        if let Some(name) = self.names.get(&key) {
            if let Ok(src) = decode(wic, &file) {
                return Some((name.clone(), src));
            }
        }
        // Cache miss: ask the shell once, then save.
        let (name, hbmp) = apps::extract(id)?;
        let result = unsafe {
            let bmp = wic.CreateBitmapFromHBITMAP(hbmp, HPALETTE::default(), WICBitmapUseAlpha).ok();
            let _ = DeleteObject(hbmp.into());
            bmp
        }?;
        let _ = encode(wic, &result, &file);
        self.names.insert(key, name.clone());
        self.dirty = true;
        let src = to_pbgra(wic, &result.into()).ok()?;
        Some((name, src))
    }

    pub fn flush(&mut self) {
        if self.dirty {
            if let Ok(t) = serde_json::to_string_pretty(&self.names) {
                let _ = std::fs::write(self.dir.join("names.json"), t);
            }
            if let Ok(t) = serde_json::to_string_pretty(&self.targets) {
                let _ = std::fs::write(self.dir.join("targets.json"), t);
            }
            self.dirty = false;
        }
    }
}

/// Off the UI thread: ask the shell for an app's icon and save it into the cache folder.
/// Returns the display name. Used by the background worker for Launchpad's many apps.
pub fn extract_to_cache(wic: &IWICImagingFactory, dir: &std::path::Path, id: &str) -> Option<String> {
    let file = dir.join(format!("{:016x}.png", fnv(id)));
    let (name, hbmp) = apps::extract(id)?;
    if !file.exists() {
        let bmp = unsafe {
            let b = wic.CreateBitmapFromHBITMAP(hbmp, HPALETTE::default(), WICBitmapUseAlpha).ok();
            let _ = DeleteObject(hbmp.into());
            b
        }?;
        let _ = encode(wic, &bmp, &file);
    } else {
        unsafe {
            let _ = DeleteObject(hbmp.into());
        }
    }
    Some(name)
}

fn to_pbgra(wic: &IWICImagingFactory, src: &IWICBitmapSource) -> Result<IWICBitmapSource> {
    unsafe {
        let conv = wic.CreateFormatConverter()?;
        conv.Initialize(src, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
        // Decode now so the file handle is released immediately.
        let bmp = wic.CreateBitmapFromSource(&conv, WICBitmapCacheOnLoad)?;
        Ok(bmp.into())
    }
}

fn decode(wic: &IWICImagingFactory, file: &PathBuf) -> Result<IWICBitmapSource> {
    unsafe {
        let dec = wic.CreateDecoderFromFilename(
            &HSTRING::from(file.as_os_str()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = dec.GetFrame(0)?;
        to_pbgra(wic, &frame.into())
    }
}

fn encode(wic: &IWICImagingFactory, bmp: &IWICBitmap, file: &PathBuf) -> Result<()> {
    unsafe {
        let stream = wic.CreateStream()?;
        stream.InitializeFromFilename(&HSTRING::from(file.as_os_str()), GENERIC_WRITE.0)?;
        let enc = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        enc.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        enc.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
        let frame = frame.unwrap();
        frame.Initialize(None)?;
        frame.WriteSource(bmp, std::ptr::null())?;
        frame.Commit()?;
        enc.Commit()?;
        Ok(())
    }
}
