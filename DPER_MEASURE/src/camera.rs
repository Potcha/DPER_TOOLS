//! Couche UVC : capture dans un thread dédié ; tampon borné pour éviter la latence cumulée.
use nokhwa::{pixel_format::RgbFormat, utils::{CameraControl, CameraFormat, CameraIndex,
    ControlValueDescription, ControlValueSetter, KnownCameraControl, KnownCameraControlFlag,
    RequestedFormat, RequestedFormatType}, Camera};
use nokhwa::FormatDecoder;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub frame_rate: u32,
    pub rgb: Vec<u8>,
}
#[derive(Clone)]
pub struct CameraMode {
    pub format: CameraFormat,
    pub label: String,
}
#[derive(Clone, Debug)]
pub enum CameraControlValue {
    Integer { value: i64, range: Option<(i64, i64)>, step: i64 },
    Float { value: f64, range: Option<(f64, f64)>, step: f64 },
    Boolean { value: bool },
    Enum { value: i64, options: Vec<i64> },
    Text { value: String },
    ReadOnly(String),
}
#[derive(Clone, Debug)]
pub struct CameraControlSetting {
    pub id: KnownCameraControl,
    pub name: String,
    pub value: CameraControlValue,
    pub writable: bool,
    pub flags: String,
    pub active: bool,
}
pub enum CameraMessage {
    Modes(Vec<CameraMode>),
    Controls(Vec<CameraControlSetting>),
    ControlUpdated(CameraControlSetting),
    ControlError(String),
    Error(String),
}
pub struct CameraTask {
    pub receiver: Receiver<CameraMessage>,
    latest_frame: Arc<Mutex<Option<Frame>>>,
    commands: SyncSender<CameraCommand>,
    stop: Arc<AtomicBool>,
}
enum CameraCommand { SetControl(KnownCameraControl, ControlValueSetter) }
impl Drop for CameraTask {
    fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); }
}

impl CameraTask {
    /// Takes the newest decoded frame. The camera thread replaces stale pending frames.
    pub fn take_latest_frame(&self) -> Option<Frame> {
        self.latest_frame.lock().ok()?.take()
    }

    pub fn set_control(&self, id: KnownCameraControl, value: ControlValueSetter) -> Result<(), String> {
        self.commands.try_send(CameraCommand::SetControl(id, value))
            .map_err(|error| format!("Commande caméra non transmise : {error}"))
    }
}

pub fn devices() -> Vec<(CameraIndex, String, String)> {
    let Some(api) = nokhwa::native_api_backend() else { return Vec::new(); };
    nokhwa::query(api).map(|devs| devs.into_iter().map(|d| {
        (d.index().clone(), d.human_name(), d.misc().trim().to_owned())
    }).collect()).unwrap_or_default()
}

pub fn start(index: CameraIndex, requested_mode: Option<CameraFormat>) -> CameraTask {
    let (tx, rx) = sync_channel::<CameraMessage>(2);
    let (command_tx, command_rx) = sync_channel::<CameraCommand>(16);
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    let latest_frame = Arc::new(Mutex::new(None));
    let frame_slot = latest_frame.clone();
    thread::spawn(move || {
        let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let mut camera = match Camera::new(index, requested) {
            Ok(cam) => cam,
            Err(e) => { let _ = tx.try_send(CameraMessage::Error(e.to_string())); return; }
        };
        let mut formats = camera.compatible_camera_formats().unwrap_or_default().into_iter()
            .filter(|format| RgbFormat::FORMATS.contains(&format.format()))
            .collect::<Vec<_>>();
        formats.sort_by_key(|format| {
            let description = format!("{:?}", format.format()).to_ascii_uppercase();
            let transport_priority = if description.contains("MJPEG") || description.contains("MJP") {
                0_u8
            } else if description.contains("YUYV") || description.contains("YUY2") {
                1_u8
            } else {
                2_u8
            };
            let preferred_mode = !(format.width() == 1920
                && format.height() == 1080
                && format.frame_rate() == 30);
            let pixels = u64::from(format.width()) * u64::from(format.height());
            (preferred_mode, transport_priority,
                std::cmp::Reverse(pixels), std::cmp::Reverse(format.frame_rate()))
        });
        let modes = formats.iter().copied().map(|format| CameraMode {
            label: format!("{} × {} · {} i/s · {:?}", format.width(), format.height(),
                format.frame_rate(), format.format()),
            format,
        }).collect();
        let _ = tx.try_send(CameraMessage::Modes(modes));
        let selected = requested_mode.filter(|requested| formats.contains(requested))
            .or_else(|| formats.first().copied());
        if let Some(format) = selected {
            let request = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(format));
            if let Err(error) = camera.set_camera_requset(request) {
                let _ = tx.try_send(CameraMessage::Error(format!("Mode caméra indisponible : {error}")));
                return;
            }
        }
        if let Err(e) = camera.open_stream() {
            let _ = tx.try_send(CameraMessage::Error(e.to_string()));
            return;
        }
        let controls = camera.camera_controls().unwrap_or_default().iter()
            .map(camera_control_setting).collect::<Vec<_>>();
        let _ = tx.send(CameraMessage::Controls(controls));
        let frame_rate = camera.camera_format().frame_rate().max(1);
        while !signal.load(Ordering::Relaxed) {
            while let Ok(command) = command_rx.try_recv() {
                match command {
                    CameraCommand::SetControl(id, value) => match camera.set_camera_control(id, value) {
                        Ok(()) => match camera.camera_control(id) {
                            Ok(control) => { let _ = tx.send(CameraMessage::ControlUpdated(camera_control_setting(&control))); }
                            Err(error) => { let _ = tx.send(CameraMessage::ControlError(error.to_string())); }
                        },
                        Err(error) => { let _ = tx.send(CameraMessage::ControlError(error.to_string())); }
                    },
                }
            }
            match camera.frame().and_then(|buffer| buffer.decode_image::<RgbFormat>()) {
                Ok(img) => {
                    let (width, height) = img.dimensions();
                    if let Ok(mut slot) = frame_slot.lock() {
                        *slot = Some(Frame { width, height, frame_rate, rgb: img.into_raw() });
                    }
                },
                Err(e) => {
                    let _ = tx.try_send(CameraMessage::Error(e.to_string()));
                    break;
                }
            }
        }
        let _ = camera.stop_stream();
    });
    CameraTask { receiver: rx, latest_frame, commands: command_tx, stop }
}

fn camera_control_setting(control: &CameraControl) -> CameraControlSetting {
    let value = match control.description() {
        ControlValueDescription::Integer { value, step, .. } => CameraControlValue::Integer {
            value: *value, range: None, step: (*step).max(1),
        },
        ControlValueDescription::IntegerRange { min, max, value, step, .. } => CameraControlValue::Integer {
            value: *value,
            range: ((*min).saturating_add(1) <= (*max).saturating_sub(1))
                .then_some(((*min).saturating_add(1), (*max).saturating_sub(1))),
            step: (*step).max(1),
        },
        ControlValueDescription::Float { value, step, .. } => CameraControlValue::Float {
            value: *value, range: None, step: step.abs().max(0.001),
        },
        ControlValueDescription::FloatRange { min, max, value, step, .. } => CameraControlValue::Float {
            value: *value,
            range: {
                let margin = step.abs().max(1e-9);
                (*min + margin < *max - margin).then_some((*min + margin, *max - margin))
            },
            step: step.abs().max(0.001),
        },
        ControlValueDescription::Boolean { value, .. } => CameraControlValue::Boolean { value: *value },
        ControlValueDescription::Enum { value, possible, .. } => CameraControlValue::Enum {
            value: *value, options: possible.clone(),
        },
        ControlValueDescription::String { value, .. } => CameraControlValue::Text { value: value.clone() },
        other => CameraControlValue::ReadOnly(format!("{other:?}")),
    };
    let flags = control.flag().iter().map(|flag| format!("{flag:?}")).collect::<Vec<_>>().join(", ");
    let writable = !control.flag().contains(&KnownCameraControlFlag::ReadOnly)
        && !control.flag().contains(&KnownCameraControlFlag::Disabled)
        && !matches!(value, CameraControlValue::ReadOnly(_));
    CameraControlSetting {
        id: control.control(), name: control.name().to_owned(), value, writable,
        flags, active: control.active(),
    }
}

enum RecorderMessage { Frame(Vec<u8>), Stop }

/// Streams bounded RGB frames to a local FFmpeg process without blocking the UI thread.
pub struct VideoRecorder {
    sender: SyncSender<RecorderMessage>,
    worker: Option<JoinHandle<Result<u64, String>>>,
    dropped_frames: Arc<AtomicU64>,
    expected_rgb_len: usize,
}

impl VideoRecorder {
    pub fn start(path: &Path, width: u32, height: u32, frame_rate: u32) -> Result<Self, String> {
        let expected_rgb_len = (width as usize).checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(3))
            .filter(|length| *length > 0)
            .ok_or_else(|| "Dimensions invalides pour l'enregistrement vidéo.".to_owned())?;
        let frame_rate = frame_rate.max(1);
        let dimensions = format!("{width}x{height}");
        let rate = frame_rate.to_string();
        let output = path.to_string_lossy().into_owned();
        let mut child = Command::new("ffmpeg")
            .arg("-y").arg("-hide_banner").arg("-loglevel").arg("error")
            .arg("-f").arg("rawvideo").arg("-pixel_format").arg("rgb24")
            .arg("-video_size").arg(&dimensions).arg("-framerate").arg(&rate)
            .arg("-i").arg("pipe:0")
            .arg("-vf").arg("crop=trunc(iw/2)*2:trunc(ih/2)*2")
            .arg("-an").arg("-c:v").arg("libx264").arg("-preset").arg("ultrafast")
            .arg("-crf").arg("18").arg("-pix_fmt").arg("yuv420p")
            .arg("-movflags").arg("+faststart").arg(&output)
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("FFmpeg indisponible; vidéo MP4 non démarrée : {error}"))?;
        let Some(mut stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Le flux d'entrée FFmpeg n'a pas pu être ouvert.".into());
        };
        let (sender, receiver) = sync_channel::<RecorderMessage>(2);
        let dropped_frames = Arc::new(AtomicU64::new(0));
        let worker = thread::spawn(move || {
            let mut written_frames = 0u64;
            while let Ok(message) = receiver.recv() {
                match message {
                    RecorderMessage::Frame(rgb) => {
                        if let Err(error) = stdin.write_all(&rgb) {
                            drop(stdin);
                            let _ = child.kill();
                            let stderr = child.wait_with_output().ok()
                                .map(|output| String::from_utf8_lossy(&output.stderr).trim().to_owned())
                                .filter(|value| !value.is_empty()).unwrap_or_default();
                            return Err(if stderr.is_empty() {
                                format!("Écriture du flux vidéo impossible : {error}")
                            } else { format!("Écriture vidéo impossible : {error} · {stderr}") });
                        }
                        written_frames = written_frames.saturating_add(1);
                    }
                    RecorderMessage::Stop => break,
                }
            }
            drop(stdin);
            let output = child.wait_with_output()
                .map_err(|error| format!("Fermeture de FFmpeg impossible : {error}"))?;
            if !output.status.success() {
                return Err(format!("FFmpeg a échoué : {}", String::from_utf8_lossy(&output.stderr).trim()));
            }
            Ok(written_frames)
        });
        Ok(Self { sender, worker: Some(worker), dropped_frames, expected_rgb_len })
    }

    /// Return true when the frame entered the bounded queue; full queues drop new frames.
    pub fn submit_frame(&self, frame: &Frame) -> bool {
        if frame.rgb.len() != self.expected_rgb_len { return false; }
        match self.sender.try_send(RecorderMessage::Frame(frame.rgb.clone())) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.dropped_frames.fetch_add(1, Ordering::Relaxed);
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    pub fn finish(mut self) -> Result<(u64, u64), String> {
        let _ = self.sender.send(RecorderMessage::Stop);
        let worker = self.worker.take().ok_or_else(|| "Tâche d'enregistrement déjà fermée.".to_owned())?;
        let written = worker.join().map_err(|_| "La tâche d'enregistrement vidéo s'est arrêtée anormalement.".to_owned())??;
        Ok((written, self.dropped_frames.load(Ordering::Relaxed)))
    }
}

impl Drop for VideoRecorder {
    fn drop(&mut self) {
        if self.worker.is_some() { let _ = self.sender.try_send(RecorderMessage::Stop); }
    }
}
