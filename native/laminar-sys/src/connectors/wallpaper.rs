//! Bing's image of the day, saved to %LOCALAPPDATA%\Laminar\wallpaper\YYYYMMDD.jpg, and
//! [`apply_wallpaper`] to set an image as the desktop background.

use super::common::*;
use crate::feed::*;
use crate::http;
use crate::util::wide;
use serde_json::Value;
use std::path::PathBuf;

pub(super) static INFOS: &[ConnectorInfo] = &[ConnectorInfo {
    id: "wallpaper",
    name: "Bing image of the day",
    about: "Downloads Bing's daily photo (1920×1080) to your PC for the wallpaper widget; no key needed.",
    help_url: "https://www.bing.com/",
    fields: &[],
    refresh_secs: 21_600,
}];

/// What the archive says about today's image.
#[derive(Debug, PartialEq)]
struct BingImage {
    date: String,
    url: String,
    title: String,
    copyright: String,
}

fn parse_bing(v: &Value) -> Option<BingImage> {
    let i = &v["images"][0];
    let date = i["startdate"].as_str().filter(|d| d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit()))?.to_string();
    let url = match i["urlbase"].as_str() {
        Some(b) => format!("https://www.bing.com{b}_1920x1080.jpg"),
        None => format!("https://www.bing.com{}", i["url"].as_str()?),
    };
    let copyright = i["copyright"].as_str().unwrap_or("").to_string();
    // Older responses have an empty title; fall back to the caption before "(©…".
    let title = i["title"].as_str().filter(|t| !t.is_empty()).map(String::from).unwrap_or_else(|| copyright.split(" (©").next().unwrap_or("").trim().to_string());
    Some(BingImage { date, url, title, copyright })
}

fn folder() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("Laminar").join("wallpaper"))
}

pub fn wallpaper(_p: &Params) -> Result<Feed, FeedError> {
    let v = get_json("https://www.bing.com/HPImageArchive.aspx?format=js&idx=0&n=1&mkt=en-US", &[], "Bing")?;
    let img = parse_bing(&v).ok_or_else(|| FeedError::Failed("Bing: unexpected response".into()))?;
    let dir = folder().ok_or_else(|| FeedError::Failed("Bing: no LOCALAPPDATA folder".into()))?;
    let path = dir.join(format!("{}.jpg", img.date));
    if !path.exists() {
        let r = http::get(&img.url, &[]).map_err(|e| net("Bing", e))?;
        check(&r, "Bing")?;
        if r.body.len() < 1024 || !r.body.starts_with(&[0xFF, 0xD8]) {
            return Err(FeedError::Failed("Bing: the image download wasn't a JPEG".into()));
        }
        std::fs::create_dir_all(&dir).map_err(|_| FeedError::Failed("Bing: can't create the wallpaper folder".into()))?;
        // Write to a temp name first so a half-written file is never picked up.
        let tmp = path.with_extension("part");
        std::fs::write(&tmp, &r.body).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|_| FeedError::Failed("Bing: can't save the image".into()))?;
    }
    let mut f = Feed { text: Some(img.title.clone()), caption: "Bing image of the day".into(), ..Default::default() };
    f.extra.insert("image_path".into(), path.to_string_lossy().into_owned());
    f.extra.insert("title".into(), img.title);
    f.extra.insert("copyright".into(), img.copyright);
    f.extra.insert("date".into(), img.date);
    Ok(f)
}

/// Sets `path` (an image file) as the desktop wallpaper for the current user and saves
/// it to the profile (`SystemParametersInfoW(SPI_SETDESKWALLPAPER)` with
/// `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`). Returns false if the file is missing or
/// Windows refuses.
pub fn apply_wallpaper(path: &str) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER};
    if !std::path::Path::new(path).is_file() {
        return false;
    }
    let mut w = wide(path);
    unsafe { SystemParametersInfoW(SPI_SETDESKWALLPAPER, 0, Some(w.as_mut_ptr() as *mut _), SPIF_UPDATEINIFILE | SPIF_SENDCHANGE).is_ok() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bing() {
        let v: Value = serde_json::from_str(
            r#"{"images":[{"startdate":"20261114","fullstartdate":"202611140800","enddate":"20261115",
              "url":"/th?id=OHR.Owl_EN-US123_1920x1080.jpg&rf=LaDigue_1920x1080.jpg&pid=hp","urlbase":"/th?id=OHR.Owl_EN-US123",
              "copyright":"Snowy owl in flight (© Someone/Getty Images)","title":"Silent wings"}]}"#,
        )
        .unwrap();
        let b = parse_bing(&v).unwrap();
        assert_eq!(b.date, "20261114");
        assert_eq!(b.url, "https://www.bing.com/th?id=OHR.Owl_EN-US123_1920x1080.jpg");
        assert_eq!(b.title, "Silent wings");
        let v: Value = serde_json::from_str(r#"{"images":[{"startdate":"20261114","url":"/a.jpg","copyright":"A lake (© X)","title":""}]}"#).unwrap();
        let b = parse_bing(&v).unwrap();
        assert_eq!((b.url.as_str(), b.title.as_str()), ("https://www.bing.com/a.jpg", "A lake"));
        assert!(parse_bing(&serde_json::from_str(r#"{"images":[{"startdate":"../x"}]}"#).unwrap()).is_none());
        assert!(!apply_wallpaper("Z:\\definitely\\missing.jpg"));
    }
}
