//! Times H.264 decoding through the NDK's MediaCodec on an Android device, to compare
//! with the browser's WebCodecs on the same frames. Runs as a shell executable
//! (`main.rs`) or as a foreground NativeActivity APK (`app.rs`). See README.md.

pub mod annexb;
#[cfg(target_os = "android")]
mod app;
#[cfg(target_os = "android")]
pub mod decoder;
pub mod options;
pub mod report;

/// Runs one benchmark and returns its summary; writes per-frame CSV if asked.
#[cfg(target_os = "android")]
pub fn benchmark(
    options: &options::Options,
    app_window: Option<&ndk::native_window::NativeWindow>,
) -> Result<String, String> {
    let stream = std::fs::read(&options.stream_path)
        .map_err(|error| format!("{}: {error}", options.stream_path))?;
    let units = annexb::split_access_units(&stream)?;
    if units.first().is_none_or(|unit| !unit.key) {
        return Err("the stream must start with a keyframe".into());
    }
    let (codec_name, timings) = decoder::run(options, &units, app_window)?;
    if let Some(path) = &options.csv_path {
        std::fs::write(path, report::csv(&timings)).map_err(|error| format!("{path}: {error}"))?;
    }
    Ok(format!(
        "codec {codec_name}, output {:?}\n{}",
        options.output,
        report::summary(&timings, options.warmup)
    ))
}
