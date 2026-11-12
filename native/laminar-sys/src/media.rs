//! Now playing + transport controls for whatever app owns the system media session.
//!
//! WinRT `Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager` (GSMTC):
//! the same source as the Windows volume flyout's media panel. Works for any app that
//! integrates with SMTC (Spotify, browsers, Media Player, ...). The album art thumbnail is
//! re-encoded to PNG via `Windows.Graphics.Imaging` (sources hand out JPEG or PNG).
//! Each call takes ~5-30 ms (thumbnail adds ~10-50 ms). Async ops are bounded by timeouts.

use serde::{Deserialize, Serialize};
use windows::Graphics::Imaging::{BitmapDecoder, BitmapEncoder};
use windows::Media::Control::{GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager, GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status};
use windows::Storage::Streams::{DataReader, IRandomAccessStream, IRandomAccessStreamReference, InMemoryRandomAccessStream};

/// What the current media session is playing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// True when the session's playback status is `Playing`.
    pub playing: bool,
    /// AppUserModelID / exe name of the source app (e.g. "Spotify.exe").
    pub app_id: String,
    /// Album art encoded as PNG, if the app provides one.
    pub thumbnail_png: Option<Vec<u8>>,
}

const T: u64 = 2000;

fn session() -> Option<Session> {
    let mgr: Manager = wait!(Manager::RequestAsync(), T)?;
    mgr.GetCurrentSession().ok()
}

/// Current media session info, or `None` when nothing is registered.
pub fn now_playing() -> Option<NowPlaying> {
    now_playing_inner(true)
}

/// Like [`now_playing`] but skips the (comparatively slow) thumbnail fetch/encode.
pub fn now_playing_no_thumbnail() -> Option<NowPlaying> {
    now_playing_inner(false)
}

fn now_playing_inner(thumb: bool) -> Option<NowPlaying> {
    let s = session()?;
    let app_id = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
    let playing = s.GetPlaybackInfo().and_then(|p| p.PlaybackStatus()).map(|st| st == Status::Playing).unwrap_or(false);
    let props = wait!(s.TryGetMediaPropertiesAsync(), T);
    let (mut title, mut artist, mut album, mut thumbnail_png) = (String::new(), String::new(), String::new(), None);
    if let Some(p) = props {
        title = p.Title().map(|h| h.to_string()).unwrap_or_default();
        artist = p.Artist().map(|h| h.to_string()).unwrap_or_default();
        album = p.AlbumTitle().map(|h| h.to_string()).unwrap_or_default();
        if thumb {
            thumbnail_png = p.Thumbnail().ok().and_then(|r| thumbnail_to_png(&r));
        }
    }
    Some(NowPlaying { title, artist, album, playing, app_id, thumbnail_png })
}

fn read_all(stream: &IRandomAccessStream) -> Option<Vec<u8>> {
    let size = stream.Size().ok()? as u32;
    if size == 0 {
        return None;
    }
    stream.Seek(0).ok()?;
    let input = stream.GetInputStreamAt(0).ok()?;
    let reader = DataReader::CreateDataReader(&input).ok()?;
    let n = wait!(reader.LoadAsync(size), T)?;
    let mut buf = vec![0u8; n as usize];
    reader.ReadBytes(&mut buf).ok()?;
    Some(buf)
}

fn thumbnail_to_png(r: &IRandomAccessStreamReference) -> Option<Vec<u8>> {
    let src = wait!(r.OpenReadAsync(), T)?;
    let src: IRandomAccessStream = windows::core::Interface::cast(&src).ok()?;
    let ctype = windows::core::Interface::cast::<windows::Storage::Streams::IContentTypeProvider>(&src).ok().and_then(|c| c.ContentType().ok()).map(|h| h.to_string()).unwrap_or_default();
    if ctype.eq_ignore_ascii_case("image/png") {
        let bytes = read_all(&src)?;
        if bytes.starts_with(b"\x89PNG") {
            return Some(bytes);
        }
        src.Seek(0).ok()?;
    }
    let dec = wait!(BitmapDecoder::CreateAsync(&src), T)?;
    let bmp = wait!(dec.GetSoftwareBitmapAsync(), T)?;
    let out = InMemoryRandomAccessStream::new().ok()?;
    let enc = wait!(BitmapEncoder::CreateAsync(BitmapEncoder::PngEncoderId().ok()?, &out), T)?;
    enc.SetSoftwareBitmap(&bmp).ok()?;
    wait!(enc.FlushAsync(), T)?;
    let out: IRandomAccessStream = windows::core::Interface::cast(&out).ok()?;
    read_all(&out)
}

/// Toggles play/pause on the current session. Returns true if the app accepted it.
pub fn play_pause() -> bool {
    session().and_then(|s| wait!(s.TryTogglePlayPauseAsync(), T)).unwrap_or(false)
}

/// Skips to the next track. Returns true if the app accepted it.
pub fn next() -> bool {
    session().and_then(|s| wait!(s.TrySkipNextAsync(), T)).unwrap_or(false)
}

/// Skips to the previous track. Returns true if the app accepted it.
pub fn previous() -> bool {
    session().and_then(|s| wait!(s.TrySkipPreviousAsync(), T)).unwrap_or(false)
}
