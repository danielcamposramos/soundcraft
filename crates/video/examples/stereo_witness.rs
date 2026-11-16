//! CC0 synthetic witnesses; no location data or external assets.
use soundcraft_video::isobmff::{Brand, Mp4Writer, SampleEntry, TrackConfig, WriteSample, WriterOptions};
use std::io::Cursor;

fn pattern(frame: usize, x: usize, y: usize, stereo: bool) -> [u8; 3] {
    if !stereo {
        return [30 + ((x * 3 + y * 7 + frame) % 160) as u8, 128, 128];
    }
    let eye = usize::from(x >= 20);
    let x = x % 20;
    // Both eyes encode the same presentation index, with independently displaced landmarks.
    if y < 2 && x < 7 {
        return [if frame & (1 << x) == 0 { 32 } else { 210 }, 128, 128];
    }
    if (3..9).contains(&y) && x == 5 + (frame / 12) % 7 + eye * 2 {
        return [180, 90, 150];
    }
    if y >= 9 && x == 13 + (frame / 24) % 4 - eye {
        return [110, 170, 90];
    }
    [24 + (frame % 12) as u8, 128, 128]
}

fn mjpeg(stereo: bool) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut writer = Mp4Writer::new(Cursor::new(Vec::new()), WriterOptions::new(Brand::Mp4))?;
    let mut entry = SampleEntry::jpeg(640, 192);
    if let Some(video) = &mut entry.video {
        video.stereo_mode = stereo.then_some(2);
    }
    let track = writer.add_track(TrackConfig::new(entry, 24))?;
    let matrix = soundcraft_video::convert::Matrix::new(false, false);
    for frame in 0..120 {
        let image = image::RgbImage::from_fn(640, 192, |x, y| {
            let [y, cb, cr] = pattern(frame, x as usize / 16, y as usize / 16, stereo);
            image::Rgb(matrix.rgb(y, cb, cr))
        });
        let mut data = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, 100).encode_image(&image)?;
        writer.write_sample(track, WriteSample { data: &data, duration: 1, composition_offset: 0, is_sync: true })?;
    }
    Ok(writer.finish()?.into_inner())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let folder = std::env::args().nth(1).ok_or("expected an output folder")?;
    let folder = std::path::Path::new(&folder);
    std::fs::create_dir_all(folder)?;
    for name in ["marked", "marked-mjpeg", "flat"] {
        let bytes = if name == "marked" {
            soundcraft_video::synth::h264_mp4_marked(120, 24, 40, 12, 24, Some(2), &|i, x, y| pattern(i, x, y, true))?
        } else {
            mjpeg(name == "marked-mjpeg")?
        };
        std::fs::write(folder.join(format!("{name}.mp4")), &bytes)?;
        let mut movie = soundcraft_video::Movie::open(bytes)?;
        for i in 0..120 {
            let frame = movie.frame(i)?;
            std::fs::write(folder.join(format!("{name}-{i:03}.rgba")), &frame.rgba)?;
        }
    }
    Ok(())
}
