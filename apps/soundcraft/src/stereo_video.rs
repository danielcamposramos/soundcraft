//! Desktop transport only; the shared UI owns frame selection and interpretation.
use craft_stereo_window::{Picture, StereoWindow};
use soundcraft_video::stereo::View;

#[derive(Default)]
pub struct Host {
    window: Option<StereoWindow>,
}
impl soundcraft_ui_egui::stereo_video::Host for Host {
    fn show(&mut self, picture: View) -> Result<(), String> {
        if self.window.is_none() {
            self.window = Some(
                StereoWindow::open(
                    &format!("SoundCraft — {}", soundcraft_ui_egui::localization::text("Stereo Video")),
                    "ai.storyteller.soundcraft.StereoVideo",
                )
                .map_err(|e| e.to_string())?,
            );
        }
        let window =
            self.window.as_ref().ok_or_else(|| soundcraft_ui_egui::localization::text("The stereo video window did not open.").to_owned())?;
        let frame = Picture::new(picture.width as usize, picture.height as usize, picture.rgba, picture.stereo)
            .and_then(|p| p.with_pixel_aspect_ratio(picture.sample_aspect.0, picture.sample_aspect.1))
            .map_err(|e| e.to_string())?;
        if window.show(frame) {
            Ok(())
        } else {
            Err(soundcraft_ui_egui::localization::text("The stereo video window closed before the complete picture was shown.").into())
        }
    }
    fn close(&mut self) {
        self.window = None;
    }
    fn is_open(&self) -> bool {
        self.window.as_ref().is_some_and(StereoWindow::is_open)
    }
    fn notice(&self) -> Option<&str> {
        self.window
            .as_ref()
            .filter(|w| !w.desktop_declares())
            .map(|_| soundcraft_ui_egui::localization::text("The desktop has no stereo declaration; showing the left eye only."))
    }
    fn display_settings(&mut self) -> Result<(), String> {
        let mut child = std::process::Command::new("kcmshell6")
            .arg("kcm_kscreen")
            .spawn()
            .map_err(|e| soundcraft_ui_egui::localized_format!("Could not open Display Settings: {error}", error = e))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}
