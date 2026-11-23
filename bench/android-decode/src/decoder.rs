use std::mem::MaybeUninit;
use std::thread;
use std::time::{Duration, Instant};

use ndk::hardware_buffer::HardwareBufferUsage;
use ndk::media::image_reader::{AcquireResult, ImageFormat, ImageReader};
use ndk::media::media_codec::{
    DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection,
};
use ndk::media::media_format::MediaFormat;
use ndk::native_window::NativeWindow;

use crate::annexb::AccessUnit;
use crate::options::{Options, Output};
use crate::report::FrameTiming;

/// How long a frame may stay inside the decoder before the run counts as stuck.
const STUCK_AFTER: Duration = Duration::from_secs(2);
/// A Surface may drop a frame a newer one replaced; don't wait long for those.
const SURFACE_GRACE: Duration = Duration::from_millis(200);
const IDLE_POLL: Duration = Duration::from_micros(100);

fn media<T>(what: &str, result: ndk::media_error::Result<T>) -> Result<T, String> {
    result.map_err(|error| format!("{what}: {error:?}"))
}

fn open_codec(options: &Options) -> Result<MediaCodec, String> {
    match &options.codec_name {
        Some(name) => {
            MediaCodec::from_codec_name(name).ok_or_else(|| format!("no codec named {name}"))
        }
        None => MediaCodec::from_decoder_type("video/avc")
            .ok_or_else(|| "this device has no video/avc decoder".to_string()),
    }
}

fn input_format(options: &Options) -> MediaFormat {
    let mut format = MediaFormat::new();
    format.set_str("mime", "video/avc");
    format.set_i32("width", options.width);
    format.set_i32("height", options.height);
    for (key, value) in &options.format_keys {
        format.set_i32(key, *value);
    }
    format
}

/// Feeds the stream at a live stream's pace and times every frame from queueing its
/// input to the decoder handing it back (and, with a Surface, to the Surface's
/// consumer receiving it), the way a streaming client would see it.
struct Session<'a> {
    codec: &'a MediaCodec,
    reader: Option<&'a ImageReader>,
    /// Whether decoded frames go on to a Surface rather than back to us.
    render: bool,
    interval_us: u64,
    queued_at: Vec<Option<Instant>>,
    timings: Vec<Option<FrameTiming>>,
}

impl Session<'_> {
    fn frame_index(&self, presentation_us: u64) -> Option<usize> {
        let index = (presentation_us / self.interval_us) as usize;
        (index < self.queued_at.len()).then_some(index)
    }

    fn queue(&mut self, index: usize, unit: &AccessUnit) -> Result<(), String> {
        let deadline = Instant::now() + STUCK_AFTER;
        let mut input = loop {
            match media(
                "dequeue input",
                self.codec.dequeue_input_buffer(Duration::ZERO),
            )? {
                DequeuedInputBufferResult::Buffer(buffer) => break buffer,
                DequeuedInputBufferResult::TryAgainLater => {
                    if Instant::now() > deadline {
                        return Err(format!("frame {index}: no free input buffer"));
                    }
                    self.drain()?;
                    thread::sleep(IDLE_POLL);
                }
            }
        };
        let buffer = input.buffer_mut();
        if buffer.len() < unit.data.len() {
            return Err(format!(
                "frame {index} is {} bytes, the input buffer holds {}",
                unit.data.len(),
                buffer.len()
            ));
        }
        for (slot, byte) in buffer.iter_mut().zip(unit.data) {
            *slot = MaybeUninit::new(*byte);
        }
        self.queued_at[index] = Some(Instant::now());
        self.timings[index] = Some(FrameTiming {
            output_ms: f64::NAN,
            image_ms: None,
            key: unit.key,
            bytes: unit.data.len(),
        });
        media(
            "queue input",
            self.codec.queue_input_buffer(
                input,
                0,
                unit.data.len(),
                index as u64 * self.interval_us,
                0,
            ),
        )
    }

    /// Collects every output and Surface image that is ready; returns whether any was.
    fn drain(&mut self) -> Result<bool, String> {
        let mut progressed = false;
        loop {
            match media(
                "dequeue output",
                self.codec.dequeue_output_buffer(Duration::ZERO),
            )? {
                DequeuedOutputBufferInfoResult::Buffer(output) => {
                    let now = Instant::now();
                    let presentation_us = output.info().presentation_time_us() as u64;
                    if let Some(index) = self.frame_index(presentation_us) {
                        let queued = self.queued_at[index].expect("output before its input");
                        if let Some(timing) = self.timings[index].as_mut() {
                            timing.output_ms = (now - queued).as_secs_f64() * 1000.0;
                        }
                    }
                    media(
                        "release output",
                        self.codec.release_output_buffer(output, self.render),
                    )?;
                    progressed = true;
                }
                DequeuedOutputBufferInfoResult::OutputFormatChanged
                | DequeuedOutputBufferInfoResult::OutputBuffersChanged => progressed = true,
                DequeuedOutputBufferInfoResult::TryAgainLater => break,
            }
        }
        let Some(reader) = self.reader else {
            return Ok(progressed);
        };
        while let AcquireResult::Image(image) = media("acquire image", reader.acquire_next_image())?
        {
            let now = Instant::now();
            let presentation_us = media("image timestamp", image.timestamp())? as u64 / 1000;
            if let Some(index) = self.frame_index(presentation_us) {
                let queued = self.queued_at[index].expect("image before its input");
                if let Some(timing) = self.timings[index].as_mut() {
                    timing.image_ms = Some((now - queued).as_secs_f64() * 1000.0);
                }
            }
            progressed = true;
        }
        Ok(progressed)
    }

    fn all_output(&self) -> bool {
        self.timings
            .iter()
            .flatten()
            .all(|timing| !timing.output_ms.is_nan())
    }

    fn all_on_surface(&self) -> bool {
        self.reader.is_none()
            || self
                .timings
                .iter()
                .flatten()
                .all(|timing| timing.image_ms.is_some())
    }

    /// Drains until `done`; returns false if that takes longer than `limit`.
    fn drain_until(&mut self, done: fn(&Self) -> bool, limit: Duration) -> Result<bool, String> {
        let deadline = Instant::now() + limit;
        while !done(self) {
            if Instant::now() > deadline {
                return Ok(false);
            }
            if !self.drain()? {
                thread::sleep(IDLE_POLL);
            }
        }
        Ok(true)
    }
}

/// Decodes `units` as a live stream would arrive. `app_window` is where
/// [`Output::Window`] presents; only the APK has one.
pub fn run(
    options: &Options,
    units: &[AccessUnit],
    app_window: Option<&NativeWindow>,
) -> Result<(String, Vec<FrameTiming>), String> {
    let codec = open_codec(options)?;
    let codec_name = media("codec name", codec.name())?;

    let reader = if options.output == Output::ImageReader {
        Some(media(
            "image reader",
            ImageReader::new_with_usage(
                options.width,
                options.height,
                ImageFormat::PRIVATE,
                HardwareBufferUsage::GPU_SAMPLED_IMAGE,
                4,
            ),
        )?)
    } else {
        None
    };
    let window = match options.output {
        Output::Buffer => None,
        Output::ImageReader => Some(media(
            "image reader window",
            reader
                .as_ref()
                .expect("reader for ImageReader output")
                .window(),
        )?),
        Output::Window => Some(
            app_window
                .ok_or("--window needs the APK, which has a window to decode onto")?
                .clone(),
        ),
    };
    media(
        "configure",
        codec.configure(
            &input_format(options),
            window.as_ref(),
            MediaCodecDirection::Decoder,
        ),
    )?;
    media("start", codec.start())?;

    let mut session = Session {
        codec: &codec,
        reader: reader.as_ref(),
        render: window.is_some(),
        interval_us: options.frame_interval.as_micros() as u64,
        queued_at: vec![None; units.len()],
        timings: (0..units.len()).map(|_| None).collect(),
    };

    let started = Instant::now();
    for (index, unit) in units.iter().enumerate() {
        let due = started + options.frame_interval * index as u32;
        while Instant::now() < due {
            if !session.drain()? {
                thread::sleep(IDLE_POLL);
            }
        }
        session.queue(index, unit)?;
    }
    if !session.drain_until(Session::all_output, STUCK_AFTER)? {
        let missing = session
            .timings
            .iter()
            .flatten()
            .filter(|timing| timing.output_ms.is_nan())
            .count();
        return Err(format!(
            "the decoder kept {missing} frames past the end of the stream"
        ));
    }
    session.drain_until(Session::all_on_surface, SURFACE_GRACE)?;
    media("stop", codec.stop())?;

    Ok((codec_name, session.timings.into_iter().flatten().collect()))
}
