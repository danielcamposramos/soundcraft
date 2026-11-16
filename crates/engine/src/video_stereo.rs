//! Per-file picture interpretation lives in the session's existing option store, never the movie.
use soundcraft_model::Session;
use soundcraft_video::stereo::Input;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Options {
    pub input: Input,
    pub swap: bool,
}

/// Reversible path encoding avoids hashes changing between Rust releases or colliding.
pub fn key(path: &str) -> String {
    use std::fmt::Write;
    let mut key = String::from("video.stereo.");
    for byte in path.bytes() {
        let _ = write!(key, "{byte:02x}");
    }
    key
}

pub fn options(session: &Session, path: &str) -> Options {
    let key = key(path);
    let id = session.edit.value(&key, 0.0) as usize;
    Options { input: Input::ALL.get(id).copied().unwrap_or_default(), swap: session.edit.flag(&format!("{key}.swap")) }
}
