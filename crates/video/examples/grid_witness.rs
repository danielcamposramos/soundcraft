//! SPDX-License-Identifier: CC0-1.0
//! Original, unmarked interleaved input witnesses; the proof chooses each file's packing.
use std::{error::Error, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let folder = std::env::args().nth(1).ok_or("expected a fresh output folder")?;
    let folder = Path::new(&folder);
    if folder.exists() {
        return Err("grid witness folder already exists; choose a fresh path".into());
    }
    let original =
        soundcraft_video::synth::h264_mp4_with(1, 1, soundcraft_video::synth::MB_W, soundcraft_video::synth::MB_H, 24, &|_, _, _| [16, 128, 128])?;
    let starts: Vec<usize> = original
        .windows(256)
        .enumerate()
        .filter_map(|(i, bytes)| (bytes.iter().all(|b| *b == 16) && (i == 0 || original[i - 1] != 16)).then_some(i))
        .collect();
    if starts.len() != soundcraft_video::synth::MB_W * soundcraft_video::synth::MB_H {
        return Err("the generated I_PCM baseline no longer has one known luma plane per macroblock".into());
    }
    std::fs::create_dir_all(folder)?;
    for mode in ["rows", "columns", "checkerboard"] {
        let mut bytes = original.clone();
        for (block, &start) in starts.iter().enumerate() {
            let bx = block % soundcraft_video::synth::MB_W;
            let by = block / soundcraft_video::synth::MB_W;
            for y in 0..16 {
                for x in 0..16 {
                    let (px, py) = (bx * 16 + x, by * 16 + y);
                    let right = match mode {
                        "rows" => py % 2 == 1,
                        "columns" => px % 2 == 1,
                        _ => (px + py) % 2 == 1,
                    };
                    // Neither value can introduce a start-code/emulation-prevention sequence.
                    bytes[start + y * 16 + x] = if right { 192 } else { 64 };
                }
            }
        }
        let mut movie = soundcraft_video::Movie::open(bytes.clone())?;
        let frame = movie.frame(0)?;
        if frame.width != 64 || frame.height != 48 || frame.sample_aspect != (1, 1) || frame.stereo_mode.is_some() {
            return Err("grid witness metadata differs from its fixed raw-frame contract".into());
        }
        std::fs::write(folder.join(format!("grid-{mode}.mp4")), bytes)?;
        std::fs::write(folder.join(format!("grid-{mode}-000.rgba")), &frame.rgba)?;
    }
    Ok(())
}
