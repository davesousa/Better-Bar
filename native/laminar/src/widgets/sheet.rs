//! `laminar.exe --widget-sheet out.png [light] [filter]`: paint every widget at every
//! size (three dock widths, small, medium and large cards) into one PNG, with sample
//! data. A quick way to review layouts without running the dock.

use std::collections::HashMap;

use windows::core::{Interface, Result, HSTRING};
use windows::Win32::Foundation::{GENERIC_WRITE, HWND};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::*;
use windows_numerics::Matrix3x2;

use super::canvas::Canvas;
use super::catalog::{self, Size};
use super::{card_size, dock_width, Ctx, Hub, Now};

pub fn run(out: &str, light: bool, filter: Option<&str>) -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        crate::ui::theme::set_light(light);

        let defs: Vec<&'static catalog::Def> = catalog::all().filter(|d| filter.map(|f| d.id.contains(f) || d.cats.iter().any(|c| format!("{c:?}").eq_ignore_ascii_case(f))).unwrap_or(true)).collect();
        let b = 54.0f32;
        let col_w = 268.0f32;
        let label_w = 150.0;
        let pad = 16.0;
        // Columns: label | dock S | dock M | dock L | small | medium | large
        let dock_cols = [Size::Small, Size::Medium, Size::Large];
        let row_h = card_size(Size::Large, col_w).1 + pad;
        let widths: Vec<f32> = defs.iter().map(|d| dock_cols.iter().map(|s| dock_width(d, *s, b) + pad).sum::<f32>()).collect();
        let dock_w = widths.iter().cloned().fold(0.0, f32::max);
        let small_w = card_size(Size::Small, col_w).0;
        let w = (label_w + dock_w + small_w + col_w * 2.0 + pad * 4.0).ceil() as u32;
        let h = (defs.len() as f32 * row_h + pad).ceil() as u32;

        let bmp = wic.CreateBitmap(w, h, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnDemand)?;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            ..Default::default()
        };
        let base: &ID2D1Factory = &factory;
        let rt = base.CreateWicBitmapRenderTarget(&bmp, &props)?;
        let dc: ID2D1DeviceContext = rt.cast()?;

        let hub = Hub::new(HWND::default());
        let weather = sample_weather();
        let stats = laminar_sys::stats::Stats { cpu_percent: 23.0, mem_used_gb: 21.3, mem_total_gb: 32.0, disk_c_used_percent: 74.0, net_down_bps: 2_400_000.0, net_up_bps: 310_000.0 };
        let control = crate::services::ControlState {
            volume: Some(0.42),
            muted: Some(false),
            wifi: Some(true),
            bluetooth: Some(true),
            dark: true,
            media: Some(laminar_sys::media::NowPlaying { title: "Midnight Drive".into(), artist: "The Long Roads".into(), album: String::new(), playing: true, app_id: String::new(), thumbnail_png: None }),
        };
        let cfg = crate::config::Config::default();
        let names: HashMap<String, String> = HashMap::new();
        let pinned = vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string(), "e".to_string()];
        let tray: Vec<(u64, String)> = (1..=14).map(|k| (k, format!("App {k}"))).collect();
        let update = laminar_sys::update::UpdateStatus::Pending(3);
        let ctx = Ctx {
            hub: &hub,
            cfg: &cfg,
            weather: Some(&weather),
            stats: Some(&stats),
            control: &control,
            pinned: &pinned,
            names: &names,
            whatsapp: Some(true),
            tray: &tray,
            hover_tray: None,
            update: Some(&update),
            sample: true,
            now: Now::get(),
        };

        dc.BeginDraw();
        dc.Clear(Some(&if light { D2D1_COLOR_F { r: 0.86, g: 0.87, b: 0.9, a: 1.0 } } else { D2D1_COLOR_F { r: 0.11, g: 0.11, b: 0.14, a: 1.0 } }));
        let ink = if light { D2D1_COLOR_F { r: 0.1, g: 0.1, b: 0.12, a: 1.0 } } else { D2D1_COLOR_F { r: 0.9, g: 0.9, b: 0.92, a: 1.0 } };
        let brush = dc.CreateSolidColorBrush(&ink, None)?;
        let icon_brush = dc.CreateSolidColorBrush(&D2D1_COLOR_F { r: 0.35, g: 0.55, b: 0.95, a: 0.9 }, None)?;
        let fmt = dw.CreateTextFormat(&HSTRING::from("Segoe UI Variable Text"), None, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 12.0, &HSTRING::from("en-us"))?;
        for (row, def) in defs.iter().enumerate() {
            let y = pad + row as f32 * row_h;
            dc.SetTransform(&Matrix3x2::identity());
            let name: Vec<u16> = format!("{}\n{}", def.name, def.id).encode_utf16().collect();
            dc.DrawText(&name, &fmt, &D2D_RECT_F { left: pad, top: y, right: label_w, bottom: y + 60.0 }, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);
            let mut x = label_w;
            let mut cells: Vec<(f32, f32, f32, f32, bool)> = Vec::new();
            for s in dock_cols {
                let ww = dock_width(def, s, b);
                cells.push((x, y, ww, b, true));
                x += ww + pad;
            }
            x = label_w + dock_w + pad;
            for s in [Size::Small, Size::Medium, Size::Large] {
                let (cw, ch) = card_size(s, col_w);
                cells.push((x, y, cw, ch, false));
                x += cw + pad;
            }
            for (cx, cy, cw, ch, dense) in cells {
                dc.SetTransform(&Matrix3x2::translation(cx, cy));
                let canvas = Canvas::new(&dc, &factory, &dw, cw, ch, 1.0, light);
                let painted = super::paint::paint(&canvas, def, &ctx, dense, &wic);
                drop(canvas);
                dc.SetTransform(&Matrix3x2::translation(cx, cy));
                for (ix, iy, isz, _) in &painted.icons {
                    dc.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: *ix, top: *iy, right: ix + isz, bottom: iy + isz }, radiusX: isz * 0.22, radiusY: isz * 0.22 }, &icon_brush);
                }
            }
        }
        dc.EndDraw(None, None)?;

        let stream = wic.CreateStream()?;
        stream.InitializeFromFilename(&HSTRING::from(out), GENERIC_WRITE.0)?;
        let enc = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        enc.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        enc.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
        let frame = frame.unwrap();
        frame.Initialize(None)?;
        frame.WriteSource(&bmp, std::ptr::null())?;
        frame.Commit()?;
        enc.Commit()?;
        println!("{} widgets -> {out} ({w}x{h})", defs.len());
        Ok(())
    }
}

fn sample_weather() -> laminar_sys::weather::Weather {
    laminar_sys::weather::Weather {
        temp_c: 17.0,
        code: 2,
        description: "Partly cloudy".into(),
        high_c: 21.0,
        low_c: 11.0,
        hourly: (0..8).map(|i| ((13 + i) as u8 % 24, 17.0 + i as f32 * 0.5, if i % 3 == 0 { 1 } else { 2 })).collect(),
        feels_c: 16.0,
        humidity: 62.0,
        wind_kmh: 14.0,
        uv: 4.0,
        is_day: true,
        sunrise: "07:02".into(),
        sunset: "16:58".into(),
        daily: (0..7).map(|i| (format!("2026-11-{:02}", 15 + i), 18.0 + i as f32, 9.0 + i as f32 * 0.5, [0, 2, 3, 61, 1, 2, 71][i])).collect(),
        now: "2026-11-15T13:20".into(),
    }
}
