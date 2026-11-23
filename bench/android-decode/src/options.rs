use std::time::Duration;

pub const USAGE: &str = "\
usage: voxl-android-decode <stream.h264> --size WxH [options]

  --size WxH          coded size of the stream (required)
  --codec NAME        a codec by name, e.g. c2.android.avc.decoder (default: the
                      platform's first video/avc decoder)
  --fps N             feed frames at this rate, like a live stream (default 30)
  --warmup N          frames left out of the statistics (default 10)
  --buffer            decode into byte buffers instead of a Surface
  --window            decode onto the app's own window (the APK only)
  --low-latency       set KEY_LOW_LATENCY (Android 11+)
  --set KEY=INT       any extra integer MediaFormat key, repeatable
  --csv PATH          write each frame's latencies here";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// Byte buffers the decoder copies frames into.
    Buffer,
    /// An offscreen `AImageReader` Surface, which also times frames reaching it.
    ImageReader,
    /// The activity's window, as a native client would present.
    Window,
}

pub struct Options {
    pub stream_path: String,
    pub width: i32,
    pub height: i32,
    pub codec_name: Option<String>,
    pub frame_interval: Duration,
    pub warmup: usize,
    pub output: Output,
    pub format_keys: Vec<(String, i32)>,
    pub csv_path: Option<String>,
}

fn parse_size(text: &str) -> Result<(i32, i32), String> {
    let (width, height) = text
        .split_once('x')
        .ok_or_else(|| format!("--size wants WxH, got {text}"))?;
    let width = width.parse().map_err(|_| format!("bad width in {text}"))?;
    let height = height
        .parse()
        .map_err(|_| format!("bad height in {text}"))?;
    Ok((width, height))
}

fn parse_key(text: &str) -> Result<(String, i32), String> {
    let (key, value) = text
        .split_once('=')
        .ok_or_else(|| format!("--set wants KEY=INT, got {text}"))?;
    let value = value
        .parse()
        .map_err(|_| format!("--set {key} wants an integer, got {value}"))?;
    Ok((key.to_string(), value))
}

pub fn parse(arguments: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut stream_path = None;
    let mut size = None;
    let mut codec_name = None;
    let mut fps = 30.0_f64;
    let mut warmup = 10;
    let mut output = Output::ImageReader;
    let mut format_keys = Vec::new();
    let mut csv_path = None;

    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        let mut value = |name: &str| {
            arguments
                .next()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match argument.as_str() {
            "--size" => size = Some(parse_size(&value("--size")?)?),
            "--codec" => codec_name = Some(value("--codec")?),
            "--fps" => {
                fps = value("--fps")?
                    .parse()
                    .map_err(|_| "--fps wants a number".to_string())?
            }
            "--warmup" => {
                warmup = value("--warmup")?
                    .parse()
                    .map_err(|_| "--warmup wants a number".to_string())?
            }
            "--buffer" => output = Output::Buffer,
            "--window" => output = Output::Window,
            "--low-latency" => format_keys.push(("low-latency".to_string(), 1)),
            "--set" => format_keys.push(parse_key(&value("--set")?)?),
            "--csv" => csv_path = Some(value("--csv")?),
            "--help" | "-h" => return Err(USAGE.to_string()),
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            other => stream_path = Some(other.to_string()),
        }
    }

    let (width, height) = size.ok_or("--size is required")?;
    if fps <= 0.0 {
        return Err("--fps must be positive".into());
    }
    Ok(Options {
        stream_path: stream_path.ok_or("no stream given")?,
        width,
        height,
        codec_name,
        frame_interval: Duration::from_secs_f64(1.0 / fps),
        warmup,
        output,
        format_keys,
        csv_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(text: &str) -> impl Iterator<Item = String> {
        text.split_whitespace()
            .map(String::from)
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn parses_everything() {
        let options = parse(arguments(
            "s.h264 --size 1280x720 --fps 60 --low-latency --set priority=0 --buffer",
        ))
        .unwrap();
        assert_eq!((options.width, options.height), (1280, 720));
        assert_eq!(options.output, Output::Buffer);
        assert_eq!(
            options.format_keys,
            vec![("low-latency".into(), 1), ("priority".into(), 0)]
        );
    }

    #[test]
    fn requires_size() {
        assert!(parse(arguments("s.h264")).is_err());
    }
}
