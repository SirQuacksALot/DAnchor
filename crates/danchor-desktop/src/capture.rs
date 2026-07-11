//! Desktop screen capture + hardware H.264 encode.
//!
//! Negotiates a screen-cast session via the xdg-desktop-portal `ScreenCast`
//! portal (through `ashpd`), then consumes the resulting PipeWire stream
//! directly via the `pipewire` crate rather than GStreamer's own
//! `pipewiresrc` element - `pipewiresrc` rejects portal-provided streams on
//! this compositor with a "target not found" error, a known unresolved
//! upstream issue also seen in other apps (not specific to this project).
//! Raw frames are handed to GStreamer via `appsrc` for hardware H.264
//! encoding (`vah264enc`, VA-API), and pulled back out via `appsink`.
//!
//! Like `discovery`'s mdns backend and `input`'s uinput sink in
//! `danchor-core`, this is real OS/hardware-boundary glue (portal D-Bus
//! calls, a live PipeWire stream, a GStreamer pipeline) with no meaningful
//! pure logic to extract and unit-test separately - not covered by unit
//! tests for that reason.

use std::fmt;
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex};

use ashpd::desktop::PersistMode;
use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSinkCallbacks, AppSrc};
use pipewire as pw;
use pw::{properties::properties, spa};

/// One encoded H.264 access unit, ready to fragment via
/// `protocol::fragment_frame` and send over the wire.
pub struct EncodedFrame {
    pub data: Vec<u8>,
    pub keyframe: bool,
}

#[derive(Debug)]
pub enum CaptureError {
    Portal(ashpd::Error),
    NoStreamSize,
    Gstreamer(String),
    Pipewire(pw::Error),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Portal(e) => write!(f, "screen-cast portal error: {e}"),
            Self::NoStreamSize => write!(f, "portal did not report a stream size"),
            Self::Gstreamer(e) => write!(f, "gstreamer error: {e}"),
            Self::Pipewire(e) => write!(f, "pipewire error: {e}"),
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<ashpd::Error> for CaptureError {
    fn from(e: ashpd::Error) -> Self {
        Self::Portal(e)
    }
}

impl From<pw::Error> for CaptureError {
    fn from(e: pw::Error) -> Self {
        Self::Pipewire(e)
    }
}

impl From<gstreamer::glib::BoolError> for CaptureError {
    fn from(e: gstreamer::glib::BoolError) -> Self {
        Self::Gstreamer(e.to_string())
    }
}

impl From<gstreamer::StateChangeError> for CaptureError {
    fn from(e: gstreamer::StateChangeError) -> Self {
        Self::Gstreamer(e.to_string())
    }
}

/// A running capture session. Dropping/`stop`ping it tears down both the
/// PipeWire stream and the GStreamer encode pipeline.
pub struct CaptureSession {
    terminate_tx: pw::channel::Sender<()>,
    pw_thread: Option<std::thread::JoinHandle<()>>,
    pipeline: Arc<Mutex<Option<gstreamer::Pipeline>>>,
}

impl CaptureSession {
    /// Negotiates the portal session (a one-shot dedicated Tokio runtime,
    /// torn down once negotiation completes) and starts capturing. `on_frame`
    /// is invoked from GStreamer's own streaming thread for every encoded
    /// access unit, once the PipeWire stream has finished format negotiation
    /// and the encode pipeline is up.
    pub fn start(
        on_frame: impl FnMut(EncodedFrame) + Send + 'static,
    ) -> Result<Self, CaptureError> {
        let portal_stream = {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| CaptureError::Gstreamer(e.to_string()))?;
            runtime.block_on(negotiate_portal())?
        };

        let pipeline_slot: Arc<Mutex<Option<gstreamer::Pipeline>>> = Arc::new(Mutex::new(None));
        let (terminate_tx, terminate_rx) = pw::channel::channel::<()>();

        let pipeline_slot_for_thread = pipeline_slot.clone();
        let pw_thread = std::thread::Builder::new()
            .name("danchor-capture-pw".into())
            .spawn(move || {
                if let Err(e) = run_pipewire_loop(
                    portal_stream,
                    terminate_rx,
                    pipeline_slot_for_thread,
                    on_frame,
                ) {
                    eprintln!("screen capture: pipewire loop exited with an error: {e}");
                }
            })
            .expect("failed to spawn the capture PipeWire thread");

        Ok(Self {
            terminate_tx,
            pw_thread: Some(pw_thread),
            pipeline: pipeline_slot,
        })
    }

    /// Stops the PipeWire stream and, if the encode pipeline had been built
    /// (format negotiation completed), tears it down too.
    pub fn stop(mut self) {
        let _ = self.terminate_tx.send(());
        if let Some(handle) = self.pw_thread.take() {
            let _ = handle.join();
        }
        if let Some(pipeline) = self.pipeline.lock().unwrap().take() {
            let _ = pipeline.set_state(gstreamer::State::Null);
        }
    }
}

struct PortalStream {
    node_id: u32,
    width: i32,
    height: i32,
    fd: OwnedFd,
}

async fn negotiate_portal() -> Result<PortalStream, CaptureError> {
    let proxy = Screencast::new().await?;
    let session = proxy.create_session(Default::default()).await?;
    proxy
        .select_sources(
            &session,
            SelectSourcesOptions::default()
                .set_cursor_mode(CursorMode::Embedded)
                .set_sources(Some(SourceType::Monitor.into()))
                .set_multiple(false)
                .set_persist_mode(PersistMode::DoNot),
        )
        .await?;

    let response = proxy
        .start(&session, None, Default::default())
        .await?
        .response()?;
    let stream = response
        .streams()
        .first()
        .ok_or(CaptureError::NoStreamSize)?
        .to_owned();
    let (width, height) = stream.size().ok_or(CaptureError::NoStreamSize)?;
    let node_id = stream.pipe_wire_node_id();
    let fd = proxy
        .open_pipe_wire_remote(&session, Default::default())
        .await?;

    Ok(PortalStream {
        node_id,
        width,
        height,
        fd,
    })
}

/// Maps a negotiated SPA raw video format to the matching GStreamer
/// `video/x-raw` format string. `None` for anything not offered in this
/// module's format-negotiation POD (see `run_pipewire_loop`) - should never
/// happen in practice since PipeWire only ever negotiates one of the
/// formats it was offered.
fn spa_format_to_gst_format(format: spa::param::video::VideoFormat) -> Option<&'static str> {
    use spa::param::video::VideoFormat;
    match format {
        VideoFormat::RGB => Some("RGB"),
        VideoFormat::RGBA => Some("RGBA"),
        VideoFormat::RGBx => Some("RGBx"),
        VideoFormat::BGRx => Some("BGRx"),
        VideoFormat::YUY2 => Some("YUY2"),
        VideoFormat::I420 => Some("I420"),
        _ => None,
    }
}

/// Builds the GStreamer encode pipeline once the real negotiated
/// format/size is known: `appsrc(caps) ! videoconvert ! capsfilter(NV12) !
/// vah264enc ! h264parse ! appsink`, wires `on_frame` to the appsink, and
/// starts it playing. Returns the `AppSrc` so the PipeWire `process`
/// callback can push frames into it.
fn build_pipeline(
    gst_format: &str,
    width: i32,
    height: i32,
    mut on_frame: impl FnMut(EncodedFrame) + Send + 'static,
) -> Result<(gstreamer::Pipeline, AppSrc), CaptureError> {
    let pipeline = gstreamer::Pipeline::new();

    let src_caps = gstreamer::Caps::builder("video/x-raw")
        .field("format", gst_format)
        .field("width", width)
        .field("height", height)
        .field("framerate", gstreamer::Fraction::new(30, 1))
        .build();
    let appsrc = AppSrc::builder()
        .caps(&src_caps)
        .format(gstreamer::Format::Time)
        .is_live(true)
        .build();

    let convert = gstreamer::ElementFactory::make("videoconvert").build()?;
    let raw_caps = gstreamer::Caps::builder("video/x-raw")
        .field("format", "NV12")
        .build();
    let capsfilter = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &raw_caps)
        .build()?;
    let enc = gstreamer::ElementFactory::make("vah264enc").build()?;
    let parse = gstreamer::ElementFactory::make("h264parse")
        .property_from_str("config-interval", "-1")
        .build()?;
    let parsed_caps = gstreamer::Caps::builder("video/x-h264")
        .field("stream-format", "byte-stream")
        .field("alignment", "au")
        .build();
    let parse_filter = gstreamer::ElementFactory::make("capsfilter")
        .property("caps", &parsed_caps)
        .build()?;
    let appsink = AppSink::builder().build();

    let src_elem = appsrc.clone().upcast::<gstreamer::Element>();
    let sink_elem = appsink.clone().upcast::<gstreamer::Element>();
    pipeline.add_many([
        &src_elem,
        &convert,
        &capsfilter,
        &enc,
        &parse,
        &parse_filter,
        &sink_elem,
    ])?;
    gstreamer::Element::link_many([
        &src_elem,
        &convert,
        &capsfilter,
        &enc,
        &parse,
        &parse_filter,
        &sink_elem,
    ])?;

    appsink.set_callbacks(
        AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gstreamer::FlowError::Eos)?;
                let buffer = sample.buffer().ok_or(gstreamer::FlowError::Error)?;
                let keyframe = !buffer.flags().contains(gstreamer::BufferFlags::DELTA_UNIT);
                let map = buffer
                    .map_readable()
                    .map_err(|_| gstreamer::FlowError::Error)?;
                on_frame(EncodedFrame {
                    data: map.as_slice().to_vec(),
                    keyframe,
                });
                Ok(gstreamer::FlowSuccess::Ok)
            })
            .build(),
    );

    pipeline.set_state(gstreamer::State::Playing)?;
    Ok((pipeline, appsrc))
}

/// Owns the PipeWire side of the capture: connects to the portal's fd,
/// negotiates a raw video format against `node_id`, and - on the first
/// successful negotiation - builds the GStreamer encode pipeline (stashing
/// it in `pipeline_slot` so `CaptureSession::stop` can tear it down) and
/// starts pushing every subsequent frame into it.
fn run_pipewire_loop(
    portal_stream: PortalStream,
    terminate_rx: pw::channel::Receiver<()>,
    pipeline_slot: Arc<Mutex<Option<gstreamer::Pipeline>>>,
    on_frame: impl FnMut(EncodedFrame) + Send + 'static,
) -> Result<(), CaptureError> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_fd_rc(portal_stream.fd, None)?;

    let _terminate_listener = {
        let mainloop_for_quit = mainloop.clone();
        terminate_rx.attach(mainloop.loop_(), move |()| mainloop_for_quit.quit())
    };

    struct StreamState {
        appsrc: Option<AppSrc>,
        on_frame: Box<dyn FnMut(EncodedFrame) + Send>,
        pipeline_slot: Arc<Mutex<Option<gstreamer::Pipeline>>>,
    }

    let stream = pw::stream::StreamRc::new(
        core,
        "danchor-screen-capture",
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;

    let _listener = stream
        .add_local_listener_with_user_data(StreamState {
            appsrc: None,
            on_frame: Box::new(on_frame),
            pipeline_slot,
        })
        .param_changed(move |_, state, id, param| {
            if state.appsrc.is_some() {
                return; // already negotiated once - ignore renegotiation for this pass
            }
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let (media_type, media_subtype) = match spa::param::format_utils::parse_format(param) {
                Ok(v) => v,
                Err(_) => return,
            };
            if media_type != spa::param::format::MediaType::Video
                || media_subtype != spa::param::format::MediaSubtype::Raw
            {
                return;
            }
            let mut info = spa::param::video::VideoInfoRaw::default();
            if info.parse(param).is_err() {
                return;
            }
            let Some(gst_format) = spa_format_to_gst_format(info.format()) else {
                eprintln!(
                    "screen capture: unsupported negotiated format {:?}",
                    info.format()
                );
                return;
            };

            match build_pipeline(
                gst_format,
                info.size().width as i32,
                info.size().height as i32,
                std::mem::replace(&mut state.on_frame, Box::new(|_| {})),
            ) {
                Ok((pipeline, appsrc)) => {
                    *state.pipeline_slot.lock().unwrap() = Some(pipeline);
                    state.appsrc = Some(appsrc);
                }
                Err(e) => eprintln!("screen capture: failed to build encode pipeline: {e}"),
            }
        })
        .process(|stream, state| {
            let Some(appsrc) = state.appsrc.as_ref() else {
                return; // format negotiation hasn't completed yet
            };
            match stream.dequeue_buffer() {
                None => {}
                Some(mut buffer) => {
                    let datas = buffer.datas_mut();
                    if datas.is_empty() {
                        return;
                    }
                    if let Some(bytes) = datas[0].data() {
                        let gst_buffer = gstreamer::Buffer::from_mut_slice(bytes.to_vec());
                        let _ = appsrc.push_buffer(gst_buffer);
                    }
                }
            }
        })
        .register()?;

    let obj = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType,
            Id,
            spa::param::format::MediaType::Video
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype,
            Id,
            spa::param::format::MediaSubtype::Raw
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            spa::param::video::VideoFormat::RGB,
            spa::param::video::VideoFormat::RGB,
            spa::param::video::VideoFormat::RGBA,
            spa::param::video::VideoFormat::RGBx,
            spa::param::video::VideoFormat::BGRx,
            spa::param::video::VideoFormat::YUY2,
            spa::param::video::VideoFormat::I420,
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle {
                width: portal_stream.width as u32,
                height: portal_stream.height as u32,
            },
            spa::utils::Rectangle {
                width: 1,
                height: 1
            },
            spa::utils::Rectangle {
                width: 4096,
                height: 4096,
            }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: 30, denom: 1 },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction {
                num: 1000,
                denom: 1,
            }
        ),
    );
    let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map_err(|_| CaptureError::Gstreamer("failed to serialize format POD".to_string()))?
    .0
    .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&values)
        .ok_or_else(|| CaptureError::Gstreamer("failed to build format POD".to_string()))?];

    stream.connect(
        spa::utils::Direction::Input,
        Some(portal_stream.node_id),
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;

    mainloop.run();
    Ok(())
}
