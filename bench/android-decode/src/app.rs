//! The foreground APK: a NativeActivity that runs one benchmark while it is the top
//! app, so vendor performance hints that follow the foreground app apply as they
//! would to a real client.
//!
//! It reads its arguments from `args.txt` in its external files directory (relative
//! paths resolve there too), writes the summary or the error to `result.txt` beside
//! it, and exits.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent, WindowManagerFlags};
use ndk::native_window::NativeWindow;

use crate::options;

const LOG_TAG: &str = "voxl-decode";

fn resolve(directory: &Path, path: &str) -> String {
    directory.join(path).to_string_lossy().into_owned()
}

fn load_options(directory: &Path) -> Result<options::Options, String> {
    let arguments_path = directory.join("args.txt");
    let text = std::fs::read_to_string(&arguments_path)
        .map_err(|error| format!("{}: {error}", arguments_path.display()))?;
    let mut options = options::parse(text.split_whitespace().map(String::from))?;
    options.stream_path = resolve(directory, &options.stream_path);
    options.csv_path = options.csv_path.map(|path| resolve(directory, &path));
    Ok(options)
}

fn benchmark(directory: &Path, window: &NativeWindow) -> String {
    match load_options(directory).and_then(|options| crate::benchmark(&options, Some(window))) {
        Ok(summary) => summary,
        Err(message) => format!("error: {message}"),
    }
}

fn wait_for_window(app: &AndroidApp) -> Option<NativeWindow> {
    let mut window = None;
    let mut destroyed = false;
    while window.is_none() && !destroyed {
        app.poll_events(None, |event| match event {
            PollEvent::Main(MainEvent::InitWindow { .. }) => window = app.native_window(),
            PollEvent::Main(MainEvent::Destroy) => destroyed = true,
            PollEvent::Main(MainEvent::InputAvailable) => drain_input(app),
            _ => {}
        });
    }
    window
}

fn drain_input(app: &AndroidApp) {
    if let Ok(mut events) = app.input_events_iter() {
        while events.next(|_| InputStatus::Unhandled) {}
    }
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag(LOG_TAG),
    );
    app.set_window_flags(
        WindowManagerFlags::KEEP_SCREEN_ON | WindowManagerFlags::FULLSCREEN,
        WindowManagerFlags::empty(),
    );
    let directory: PathBuf = app
        .external_data_path()
        .expect("the activity has no external files directory");

    let Some(window) = wait_for_window(&app) else {
        log::error!("destroyed before a window appeared");
        return;
    };

    let (done_sender, done) = mpsc::channel();
    let waker = app.create_waker();
    let worker_directory = directory.clone();
    thread::spawn(move || {
        let result = benchmark(&worker_directory, &window);
        let _ = done_sender.send(result);
        waker.wake();
    });

    let result = loop {
        app.poll_events(None, |event| {
            if let PollEvent::Main(MainEvent::InputAvailable) = event {
                drain_input(&app);
            }
        });
        if let Ok(result) = done.try_recv() {
            break result;
        }
    };
    log::info!("{result}");
    let result_path = directory.join("result.txt");
    if let Err(error) = std::fs::write(&result_path, &result) {
        log::error!("{}: {error}", result_path.display());
    }
    std::process::exit(0);
}
