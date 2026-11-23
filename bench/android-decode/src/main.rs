use std::process::ExitCode;

use voxl_android_decode::options;

#[cfg(target_os = "android")]
fn run(options: &options::Options) -> Result<String, String> {
    voxl_android_decode::benchmark(options, None)
}

#[cfg(not(target_os = "android"))]
fn run(_options: &options::Options) -> Result<String, String> {
    Err("MediaCodec runs only on Android; build with --target aarch64-linux-android".into())
}

fn main() -> ExitCode {
    match options::parse(std::env::args().skip(1)).and_then(|options| run(&options)) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
