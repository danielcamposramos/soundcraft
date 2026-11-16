//! A decoded presentation picture is the indivisible boundary of a spatial stereo pair.
//! Samples are copied through craft-media's grid owner; playback never pairs separate jobs.
use crate::{Frame, MovieInfo, Result, VideoError};
use craft_media::stereo::{Eye, PackedGrid, Packing, SampleAspect};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Input {
    #[default]
    Auto,
    Mono,
    SideBySide,
    SideBySideRight,
    HalfSideBySide,
    HalfSideBySideRight,
    TopBottom,
    TopBottomRight,
    HalfTopBottom,
    HalfTopBottomRight,
    Rows,
    RowsRight,
    Columns,
    ColumnsRight,
    Checkerboard,
    CheckerboardRight,
}
impl Input {
    pub const ALL: [Self; 16] = [
        Self::Auto,
        Self::Mono,
        Self::SideBySide,
        Self::SideBySideRight,
        Self::HalfSideBySide,
        Self::HalfSideBySideRight,
        Self::TopBottom,
        Self::TopBottomRight,
        Self::HalfTopBottom,
        Self::HalfTopBottomRight,
        Self::Rows,
        Self::RowsRight,
        Self::Columns,
        Self::ColumnsRight,
        Self::Checkerboard,
        Self::CheckerboardRight,
    ];

    pub fn id(self) -> usize {
        Self::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "From file",
            Self::Mono => "2D",
            Self::SideBySide => "Side by side, left first",
            Self::SideBySideRight => "Side by side, right first",
            Self::HalfSideBySide => "Half side by side, left first",
            Self::HalfSideBySideRight => "Half side by side, right first",
            Self::TopBottom => "Top and bottom, left first",
            Self::TopBottomRight => "Top and bottom, right first",
            Self::HalfTopBottom => "Half top and bottom, left first",
            Self::HalfTopBottomRight => "Half top and bottom, right first",
            Self::Rows => "Rows, left first",
            Self::RowsRight => "Rows, right first",
            Self::Columns => "Columns, left first",
            Self::ColumnsRight => "Columns, right first",
            Self::Checkerboard => "Checkerboard, left first",
            Self::CheckerboardRight => "Checkerboard, right first",
        }
    }
}

#[derive(Clone, Debug)]
pub struct View {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
    pub index: usize,
    pub time: f64,
    pub stereo: bool,
    pub sample_aspect: (u32, u32),
}

/// Keep one source picture's index, timestamp, native grids and sample shape together.
/// Unmarked/explicit 2D pictures remain a single picture, never two invented eyes.
pub fn view(frame: &Frame, info: &MovieInfo, input: Input, swap: bool, budget: usize) -> Result<View> {
    let err = |s: &str| VideoError::Unsupported(s.into());
    let mut input = input;
    if input == Input::Auto && info.codec == "h264" {
        return Err(err("Automatic H.264 stereo needs codec metadata; choose this file's input packing."));
    }
    if input == Input::Auto {
        input = match frame.stereo_mode {
            None | Some(0) => Input::Mono,
            Some(1) => Input::TopBottom,
            Some(2) => Input::SideBySide,
            Some(_) => return Err(err("This file declares a stereo layout SoundCraft cannot reconstruct; choose its input packing.")),
        };
    }
    let (pixels, tail) = frame.rgba.as_chunks::<4>();
    if !tail.is_empty() || Some(pixels.len()) != (frame.width as usize).checked_mul(frame.height as usize) || frame.width == 0 || frame.height == 0 {
        return Err(err("The decoded picture does not match its native grid."));
    }
    if frame.rgba.len() > budget {
        return Err(err("The stereo picture exceeds the output byte limit."));
    }
    let mut aspect =
        SampleAspect::new(frame.sample_aspect.0, frame.sample_aspect.1).ok_or_else(|| err("The sample aspect ratio is invalid."))?.ratio();
    let (packing, mut first) = match input {
        Input::Auto | Input::Mono => {
            return Ok(View {
                width: frame.width,
                height: frame.height,
                rgba: Arc::clone(&frame.rgba),
                index: frame.index,
                time: frame.time,
                stereo: false,
                sample_aspect: aspect,
            });
        }
        Input::SideBySide | Input::HalfSideBySide => (Packing::SideBySide { gap: 0 }, Eye::Left),
        Input::SideBySideRight | Input::HalfSideBySideRight => (Packing::SideBySide { gap: 0 }, Eye::Right),
        Input::TopBottom | Input::HalfTopBottom => (Packing::TopBottom { gap: 0 }, Eye::Left),
        Input::TopBottomRight | Input::HalfTopBottomRight => (Packing::TopBottom { gap: 0 }, Eye::Right),
        Input::Rows => (Packing::Rows, Eye::Left),
        Input::RowsRight => (Packing::Rows, Eye::Right),
        Input::Columns => (Packing::Columns, Eye::Left),
        Input::ColumnsRight => (Packing::Columns, Eye::Right),
        Input::Checkerboard => (Packing::Checkerboard, Eye::Left),
        Input::CheckerboardRight => (Packing::Checkerboard, Eye::Right),
    };
    if matches!(input, Input::HalfSideBySide | Input::HalfSideBySideRight) {
        aspect.0 = aspect.0.checked_mul(2).ok_or_else(|| err("The sample aspect ratio is too large."))?;
    }
    if matches!(input, Input::HalfTopBottom | Input::HalfTopBottomRight) {
        aspect.1 = aspect.1.checked_mul(2).ok_or_else(|| err("The sample aspect ratio is too large."))?;
    }
    if swap {
        first = if first == Eye::Left { Eye::Right } else { Eye::Left };
    }
    let aspect = SampleAspect::new(aspect.0, aspect.1).ok_or_else(|| err("The sample aspect ratio is invalid."))?;
    let grid = PackedGrid::new(frame.width as usize, frame.height as usize, packing, first, aspect).map_err(|e| err(&e.to_string()))?;
    if matches!(packing, Packing::SideBySide { gap: 0 }) && first == Eye::Left {
        return Ok(View {
            width: frame.width,
            height: frame.height,
            rgba: Arc::clone(&frame.rgba),
            index: frame.index,
            time: frame.time,
            stereo: true,
            sample_aspect: aspect.ratio(),
        });
    }
    let eyes = grid.copy_eyes(pixels, budget).map_err(|e| err(&e.to_string()))?;
    let (w, h) = eyes.eye_size();
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(frame.rgba.len()).map_err(|_| err("Could not allocate the stereo picture."))?;
    for y in 0..h {
        for eye in [&eyes.left, &eyes.right] {
            for pixel in eye.iter().skip(y * w).take(w) {
                rgba.extend_from_slice(pixel);
            }
        }
    }
    Ok(View {
        width: u32::try_from(w.checked_mul(2).ok_or_else(|| err("invalid stereo sample-grid geometry"))?)
            .map_err(|_| err("invalid stereo sample-grid geometry"))?,
        height: u32::try_from(h).map_err(|_| err("invalid stereo sample-grid geometry"))?,
        rgba: rgba.into(),
        index: frame.index,
        time: frame.time,
        stereo: true,
        sample_aspect: aspect.ratio(),
    })
}
