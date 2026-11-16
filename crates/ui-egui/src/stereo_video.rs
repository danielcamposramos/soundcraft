//! The native host receives one immutable, indexed picture at the audio playhead.
//! There is no decoder or playback clock in the host.
use crate::{
    SoundApp,
    localization::text,
    video_track::{MovieKey, online, picture_at},
};
use serde_json::{Value, json};
use soundcraft_video::stereo::{Input, View};

pub trait Host {
    fn show(&mut self, picture: View) -> Result<(), String>;
    fn close(&mut self);
    fn is_open(&self) -> bool;
    fn display_settings(&mut self) -> Result<(), String>;
    fn notice(&self) -> Option<&str> {
        None
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Shown {
    pub source: u64,
    pub index: usize,
    pub time: f64,
    pub requested_secs: f64,
    pub size: [u32; 2],
    pub sample_aspect: (u32, u32),
    pub stereo: bool,
    pub input: Input,
    pub swap: bool,
}

type Key = (u64, MovieKey, usize, u64, Input, bool);
#[derive(Default)]
pub struct State {
    pub enabled: bool,
    pub options_open: bool,
    pub shown: Option<Shown>,
    pub error: Option<String>,
    notice: Option<String>,
    key: Option<Key>,
    opened: bool,
}
impl State {
    pub fn inspect(&self) -> Value {
        json!({"enabled":self.enabled,"shown":self.shown,"error":self.error,"notice":self.notice})
    }
    fn clear_error(&mut self, app: &mut SoundApp) {
        if self.error.as_ref().is_some_and(|error| *error == app.ui.status) {
            app.ui.status.clear();
        }
        self.error = None;
    }
    fn close(&mut self, host: &mut dyn Host) {
        if self.opened {
            host.close();
        }
        self.opened = false;
        self.key = None;
        self.shown = None;
        self.notice = None;
    }
    fn update(&mut self, app: &mut SoundApp, ctx: &egui::Context, host: &mut dyn Host) -> Result<(), String> {
        if !self.enabled {
            self.close(host);
            return Ok(());
        }
        if self.opened && !host.is_open() {
            self.enabled = false;
            self.close(host);
            return Ok(());
        }
        let session = app.engine.session_arc();
        if !online(&session) {
            self.close(host);
            return Err(text("Video Track Offline").into());
        }
        let Some((key, path, secs, fps)) = picture_at(&session, app.position()) else {
            self.close(host);
            self.clear_error(app);
            return Ok(());
        };
        let path = path?;
        if self.key.as_ref().is_some_and(|k| k.0 != app.doc_epoch || k.1 != key) {
            self.close(host);
        }
        if secs < 0.0 || app.video.info(key).is_some_and(|info| info.as_ref().is_ok_and(|info| secs >= info.duration)) {
            self.close(host);
            self.clear_error(app);
            return Ok(());
        }
        app.video.poll_native(ctx);
        app.video.request_frame(ctx, key, &path, secs, fps);
        if let Some(error) = app.video.current_error(key) {
            self.close(host);
            return Err(error.into());
        }
        if let Some(Err(error)) = app.video.info(key) {
            self.close(host);
            return Err(error.clone());
        }
        let Some((requested_secs, frame)) = app.video.native(key) else {
            return Ok(());
        };
        let Some(Ok(info)) = app.video.info(key) else {
            return Ok(());
        };
        let options = soundcraft_engine::video_stereo::options(&session, &path);
        let identity = (app.doc_epoch, key, frame.index, frame.time.to_bits(), options.input, options.swap);
        if self.key == Some(identity) {
            return Ok(());
        }
        let picture = soundcraft_video::stereo::view(frame, info, options.input, options.swap, 512 * 1024 * 1024).map_err(|e| match e {
            soundcraft_video::VideoError::Unsupported(message) => text(&message).to_owned(),
            other => other.to_string(),
        })?;
        let shown = Shown {
            source: key.0.0,
            index: picture.index,
            time: picture.time,
            requested_secs,
            size: [if picture.stereo { picture.width / 2 } else { picture.width }, picture.height],
            sample_aspect: picture.sample_aspect,
            stereo: picture.stereo,
            input: options.input,
            swap: options.swap,
        };
        host.show(picture)?;
        let notice = host.notice().filter(|_| shown.stereo).map(str::to_owned).or_else(|| {
            (options.input == Input::Auto && frame.stereo_mode.is_none())
                .then(|| text("Automatic packing reads MP4 st3d. Choose the packing for other marks.").to_owned())
        });
        if notice != self.notice {
            if let Some(notice) = &notice {
                app.ui.status = notice.clone();
            } else if self.notice.as_ref().is_some_and(|previous| *previous == app.ui.status) {
                app.ui.status.clear();
            }
            self.notice = notice;
        }
        self.opened = true;
        self.shown = Some(shown);
        self.key = Some(identity);
        self.clear_error(app);
        Ok(())
    }
}

pub fn command(app: &mut SoundApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    let result = match id {
        "video.stereoOptions" => {
            app.stereo_video.options_open = true;
            Ok(json!({}))
        }
        "video.stereo" => {
            let enabled = match params.get("enabled") {
                Some(value) => match serde_json::from_value::<bool>(value.clone()) {
                    Ok(enabled) => enabled,
                    Err(error) => {
                        let error = error.to_string();
                        app.ui.status = error.clone();
                        return Some(Err(error));
                    }
                },
                None => !app.stereo_video.enabled,
            };
            if enabled && app.services.stereo_video.is_none() {
                let error = text("This host has no stereo video window.").to_owned();
                app.ui.status = error.clone();
                return Some(Err(error));
            }
            app.stereo_video.enabled = enabled;
            app.stereo_video.error = None;
            Ok(json!({"enabled":enabled}))
        }
        "video.displaySettings" => app
            .services
            .stereo_video
            .as_mut()
            .ok_or_else(|| text("This host has no stereo video window.").to_owned())
            .and_then(|h| h.display_settings())
            .map(|()| json!({})),
        _ => return None,
    };
    if let Err(error) = &result {
        app.ui.status = error.clone();
    }
    Some(result)
}

pub fn step(app: &mut SoundApp, ctx: &egui::Context) {
    let mut state = std::mem::take(&mut app.stereo_video);
    if let Some(mut host) = app.services.stereo_video.take() {
        if let Err(error) = state.update(app, ctx, host.as_mut()) {
            state.close(host.as_mut());
            state.error = Some(error.clone());
            app.ui.status = error;
        }
        app.services.stereo_video = Some(host);
    }
    app.stereo_video = state;
}

pub fn show_options(app: &mut SoundApp, ctx: &egui::Context) {
    let mut state = std::mem::take(&mut app.stereo_video);
    let actions = options(app, ctx, &mut state);
    app.stereo_video = state;
    for (command, params) in actions {
        let _ = app.run(command, params);
    }
}

fn options(app: &SoundApp, ctx: &egui::Context, state: &mut State) -> Vec<(&'static str, Value)> {
    let mut actions = Vec::new();
    if !state.options_open {
        return actions;
    }
    egui::Window::new(text("Stereo Video")).open(&mut state.options_open).show(ctx, |ui| {
        let mut enabled = state.enabled;
        if ui.checkbox(&mut enabled, text("Enable 3D")).changed() {
            actions.push(("video.stereo", json!({"enabled":enabled})));
        }
        ui.label(text("Render resolution per eye"));
        if let Some(shown) = &state.shown {
            ui.label(format!("{} × {}", shown.size[0], shown.size[1]));
        }
        ui.label(text("Native film samples; the desktop scales once."));
        if ui.button(text("Display Settings")).clicked() {
            actions.push(("video.displaySettings", json!({})));
        }
        ui.separator();
        if let Some((key, Ok(path), ..)) = picture_at(app.engine.session(), app.position()) {
            let settings = soundcraft_engine::video_stereo::options(app.engine.session(), &path);
            let mut input = settings.input;
            craft_ui::ComboBox::from_label(text("Input packing")).selected_text(text(input.label())).show_ui(ui, |ui| {
                for choice in Input::ALL {
                    ui.selectable_value(&mut input, choice, text(choice.label()));
                }
            });
            let mut swap = settings.swap;
            if state.enabled && state.shown.as_ref().is_some_and(|shown| shown.source == key.0.0 && shown.stereo) {
                ui.checkbox(&mut swap, text("Swap left and right eyes"));
            }
            if (input, swap) != (settings.input, settings.swap) {
                actions.push(("video.stereoInput", json!({"source":key.0.0,"input":input,"swap":swap})));
            }
        } else {
            ui.label(text("No movie at the audio playhead."));
        }
        ui.label(text("Automatic packing reads MP4 st3d. Choose the packing for other marks."));
        if let Some(error) = &state.error {
            ui.colored_label(egui::Color32::RED, error);
        }
    });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Services;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        time::{Duration, Instant},
    };

    struct RecordingHost {
        pictures: Rc<RefCell<Vec<View>>>,
        open: Rc<Cell<bool>>,
    }
    impl Host for RecordingHost {
        fn show(&mut self, picture: View) -> Result<(), String> {
            self.pictures.borrow_mut().push(picture);
            self.open.set(true);
            Ok(())
        }
        fn close(&mut self) {
            self.open.set(false);
        }
        fn is_open(&self) -> bool {
            self.open.get()
        }
        fn display_settings(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn notice(&self) -> Option<&str> {
            Some("The desktop has no stereo declaration; showing the left eye only.")
        }
    }

    fn tick(app: &mut SoundApp, ctx: &egui::Context) {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.logic(ui.ctx()));
        output.textures_delta.clear();
    }

    fn wait_frame(app: &mut SoundApp, ctx: &egui::Context, index: usize) {
        let start = Instant::now();
        loop {
            tick(app, ctx);
            if app.stereo_video.shown.as_ref().is_some_and(|v| v.index == index) {
                return;
            }
            assert!(start.elapsed() < Duration::from_secs(10), "{:?}", app.stereo_video.inspect());
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn played_picture_seek_and_swap_keep_one_decode_identity_and_offline_closes_it() {
        let path = std::env::temp_dir().join(format!("soundcraft-stereo-{}.mp4", std::process::id()));
        let data = soundcraft_video::synth::h264_mp4_marked(12, 12, 4, 3, 24, Some(2), &|i, x, _| {
            [if x < 2 { 30 + i as u8 * 8 } else { 200 - i as u8 * 5 }, 128, 128]
        })
        .unwrap();
        std::fs::write(&path, &data).unwrap();
        let pictures = Rc::new(RefCell::new(Vec::new()));
        let open = Rc::new(Cell::new(false));
        let services =
            Services { stereo_video: Some(Box::new(RecordingHost { pictures: pictures.clone(), open: open.clone() })), ..Default::default() };
        let mut app = SoundApp::new(Default::default(), None, services);
        app.run("file.import_video", json!({"path":path})).unwrap();
        let source = app.engine.session().videos[0].id.0;
        app.run("video.stereoInput", json!({"source":source,"input":"side-by-side"})).unwrap();
        app.run("video.stereo", json!({"enabled":true})).unwrap();
        let ctx = egui::Context::default();
        app.engine.transport.playing = true;
        app.engine.transport.position = 1;
        wait_frame(&mut app, &ctx, 0);
        let rate = i64::from(app.engine.session().sample_rate.hz());
        let mut reference = soundcraft_video::Movie::open(data.clone()).unwrap();
        for index in [7usize, 2, 9] {
            app.engine.transport.position = rate * index as i64 / 24 + 1;
            wait_frame(&mut app, &ctx, index);
            let shown = app.stereo_video.shown.as_ref().unwrap();
            assert_eq!(shown.time, index as f64 / 24.0);
            assert!((shown.requested_secs - index as f64 / 24.0).abs() < 0.001);
            let expected = reference.frame(index).unwrap();
            let actual = pictures.borrow();
            let actual = actual.last().unwrap();
            assert!(actual.stereo);
            assert_eq!((actual.index, actual.time), (index, expected.time));
            assert_eq!(actual.rgba.as_ref(), expected.rgba.as_ref());
        }
        let original = pictures.borrow().last().unwrap().clone();
        app.run("video.stereoInput", json!({"source":source,"swap":true})).unwrap();
        tick(&mut app, &ctx);
        let swapped = pictures.borrow().last().unwrap().clone();
        assert_eq!((original.index, original.time), (swapped.index, swapped.time));
        for row in 0..48 {
            let offset = row * 64 * 4;
            assert_eq!(&original.rgba[offset..offset + 32 * 4], &swapped.rgba[offset + 32 * 4..offset + 64 * 4]);
        }
        app.run("video.online", json!({"value":false})).unwrap();
        tick(&mut app, &ctx);
        assert!(!open.get() && app.stereo_video.enabled && app.stereo_video.shown.is_none());
        assert_eq!(app.stereo_video.error.as_deref(), Some("Video Track Offline"));
        app.run("video.online", json!({"value":true})).unwrap();
        app.engine.session_mut().edit.values.insert("video.sync_offset".into(), 4800.0);
        app.engine.transport.position = 0;
        tick(&mut app, &ctx);
        assert!(app.stereo_video.enabled && app.stereo_video.shown.is_none());
        app.engine.transport.position = 9600;
        wait_frame(&mut app, &ctx, 2);
        assert!(open.get() && app.stereo_video.error.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), data);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_host_without_a_stereo_window_refuses_visibly() {
        let mut app = SoundApp::new(Default::default(), None, Services::default());
        assert!(app.run("video.stereo", json!({"enabled":true})).is_err());
        assert!(!app.stereo_video.enabled);
        assert_eq!(app.ui.status, "This host has no stereo video window.");
    }

    #[test]
    fn swap_is_offered_only_for_the_presented_stereo_movie() {
        use egui_kittest::{Harness, kittest::Queryable};
        use soundcraft_model::{ChannelFormat, Clip, SourceId, TrackKind, VideoSource};
        let mut app = SoundApp::new(Default::default(), None, Services::default());
        let s = app.engine.session_mut();
        let track = s.add_track(TrackKind::Video, ChannelFormat::Mono, None);
        let source = SourceId(s.alloc());
        s.videos.push(VideoSource {
            id: source,
            name: "reference".into(),
            path: "reference.mp4".into(),
            width: 640,
            height: 192,
            frame_rate: 24.0,
            duration: 1.0,
            codec: "jpeg".into(),
            offline: None,
        });
        let clip = Clip::video(s.new_clip_id(), "reference", source, 0, 0, 48_000);
        s.track_mut(track).unwrap().playlist_mut().unwrap().clips.push(clip);
        let state = State { options_open: true, ..Default::default() };
        let mut h = Harness::builder().with_size(egui::vec2(900.0, 700.0)).build_ui_state(
            |ui, (app, state): &mut (SoundApp, State)| {
                let _ = options(app, ui.ctx(), state);
            },
            (app, state),
        );
        h.step();
        assert!(h.query_by_label("Enable 3D").is_some());
        assert!(h.query_by_label("Swap left and right eyes").is_none());
        h.state_mut().1.enabled = true;
        h.state_mut().1.shown = Some(Shown {
            source: source.0,
            index: 0,
            time: 0.0,
            requested_secs: 0.0,
            size: [640, 192],
            sample_aspect: (1, 1),
            stereo: false,
            input: Input::Mono,
            swap: false,
        });
        h.step();
        assert!(h.query_by_label("Swap left and right eyes").is_none());
        let shown = h.state_mut().1.shown.as_mut().unwrap();
        shown.stereo = true;
        shown.size = [320, 192];
        shown.input = Input::SideBySide;
        h.step();
        assert!(h.query_by_label("Swap left and right eyes").is_some());
        h.state_mut().1.shown.as_mut().unwrap().source = source.0 + 1;
        h.step();
        assert!(h.query_by_label("Swap left and right eyes").is_none());
    }

    #[test]
    fn minimized_logic_keeps_native_video_live_without_ui_texture_uploads() {
        let path = std::env::temp_dir().join(format!("soundcraft-stereo-hidden-{}.mp4", std::process::id()));
        let data = soundcraft_video::synth::h264_mp4_marked(12, 12, 4, 3, 24, Some(2), &|i, x, _| {
            [if x < 2 { 30 + i as u8 * 8 } else { 200 - i as u8 * 5 }, 128, 128]
        })
        .unwrap();
        std::fs::write(&path, &data).unwrap();
        let pictures = Rc::new(RefCell::new(Vec::new()));
        let open = Rc::new(Cell::new(false));
        let services =
            Services { stereo_video: Some(Box::new(RecordingHost { pictures: pictures.clone(), open: open.clone() })), ..Default::default() };
        let mut app = SoundApp::new(Default::default(), None, services);
        app.run("file.import_video", json!({"path":path})).unwrap();
        let source = app.engine.session().videos[0].id;
        app.run("video.stereoInput", json!({"source":source.0,"input":"side-by-side"})).unwrap();
        app.run("video.stereo", json!({"enabled":true})).unwrap();
        let ctx = egui::Context::default();
        let hidden = egui::ViewportInfo { minimized: Some(true), ..Default::default() };
        let raw = egui::RawInput { viewports: [(egui::ViewportId::ROOT, hidden)].into_iter().collect(), ..Default::default() };
        let rate = i64::from(app.engine.session().sample_rate.hz());
        app.engine.transport.playing = true;
        let key = MovieKey::new(source, path.to_str().unwrap());
        for index in [0usize, 5, 9] {
            app.engine.transport.position = rate * index as i64 / 24 + 1;
            let start = Instant::now();
            loop {
                let _ = ctx.run_logic(&raw, |ctx| app.logic(ctx));
                if app.stereo_video.shown.as_ref().is_some_and(|shown| shown.index == index) {
                    break;
                }
                assert!(start.elapsed() < Duration::from_secs(10), "hidden logic did not deliver frame {index}: {:?}", app.stereo_video.inspect());
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(pictures.borrow().last().unwrap().index, index);
            assert!(app.video.current(key).is_none(), "hidden playback uploaded a UI texture");
        }
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.video.poll(ui.ctx()));
        out.textures_delta.clear();
        assert!(app.video.current(key).is_some(), "ordinary painting did not upload the latest native picture");
        assert_eq!(std::fs::read(&path).unwrap(), data);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_flat_picture_never_reports_a_left_eye_fallback() {
        let path = std::env::temp_dir().join(format!("soundcraft-stereo-mono-{}.mp4", std::process::id()));
        let data = soundcraft_video::synth::h264_mp4_marked(1, 1, 4, 3, 24, Some(2), &|_, _, _| [80, 128, 128]).unwrap();
        std::fs::write(&path, data).unwrap();
        let pictures = Rc::new(RefCell::new(Vec::new()));
        let services = Services { stereo_video: Some(Box::new(RecordingHost { pictures, open: Rc::new(Cell::new(false)) })), ..Default::default() };
        let mut app = SoundApp::new(Default::default(), None, services);
        app.run("file.import_video", json!({"path":path})).unwrap();
        let source = app.engine.session().videos[0].id.0;
        app.run("video.stereoInput", json!({"source":source,"input":"mono"})).unwrap();
        app.run("video.stereo", json!({"enabled":true})).unwrap();
        wait_frame(&mut app, &egui::Context::default(), 0);
        assert!(!app.stereo_video.shown.as_ref().unwrap().stereo);
        assert!(app.stereo_video.notice.is_none(), "a single flat picture has no left-eye fallback");
        assert_ne!(app.ui.status, "The desktop has no stereo declaration; showing the left eye only.");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn an_explicit_non_boolean_enable_never_toggles_the_choice() {
        let mut app = SoundApp::new(Default::default(), None, Services::default());
        app.stereo_video.enabled = true;
        for value in [json!("false"), Value::Null, json!(0), json!([]), json!({})] {
            assert!(app.run("video.stereo", json!({"enabled":value})).is_err());
            assert!(app.stereo_video.enabled, "a malformed enable value toggled the existing choice");
            assert!(!app.ui.status.is_empty());
        }
        app.run("video.stereo", json!({"enabled":false})).unwrap();
        assert!(!app.stereo_video.enabled);
    }
}
