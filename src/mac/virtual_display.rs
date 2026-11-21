//! A virtual display for a Mac with no screen attached (a Mac mini whose monitor is
//! switched off, or one running headless), so there is always a desktop to stream.
//!
//! It uses CoreGraphics' CGVirtualDisplay classes, which are private API: they are looked
//! up at runtime and any change Apple makes to them shows up as an error, not a crash.
//! The display lives as long as the object voxl holds; dropping it removes the display.

use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::msg_send;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSArray, NSString};
use parking_lot::Mutex;
use std::ffi::CStr;
use std::time::{Duration, Instant};

/// Modes offered besides the requested size, so dynamic resolution has sizes to pick.
const STANDARD_MODES: [(u32, u32); 10] = [
    (2560, 1440),
    (1920, 1200),
    (1920, 1080),
    (1680, 1050),
    (1600, 900),
    (1440, 900),
    (1366, 768),
    (1280, 800),
    (1280, 720),
    (1024, 768),
];

const REFRESH_RATE: f64 = 60.0;

/// Roughly a 24" panel, so apps that size things by physical size look normal.
const PIXELS_PER_MILLIMETER: f64 = 3.6;

struct VirtualDisplay {
    id: u32,
    _display: Retained<AnyObject>,
    _queue: DispatchRetained<DispatchQueue>,
}

// SAFETY: CGVirtualDisplay has no thread affinity; voxl only keeps it alive and drops it.
unsafe impl Send for VirtualDisplay {}

static CURRENT: Mutex<Option<VirtualDisplay>> = Mutex::new(None);

/// Parses `--virtual-display`: `off`, or the size as `WIDTHxHEIGHT`.
pub fn parse_size(value: &str) -> Result<Option<(u32, u32)>, String> {
    let value = value.trim();
    if matches!(value, "off" | "false" | "0") {
        return Ok(None);
    }
    let (width, height) = value
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("--virtual-display {value}: expected WIDTHxHEIGHT or off"))?;
    let parse = |s: &str| {
        s.trim()
            .parse::<u32>()
            .ok()
            .filter(|&n| (480..=8192).contains(&n))
            .ok_or_else(|| format!("--virtual-display {value}: sizes go from 480 to 8192"))
    };
    let (width, height) = (parse(width)?, parse(height)?);
    Ok(Some((width & !1, height & !1)))
}

/// The requested size first (macOS starts the display in the first mode), then the
/// standard sizes that fit inside it, largest first.
pub fn modes_for(size: (u32, u32)) -> Vec<(u32, u32)> {
    let mut modes = vec![size];
    modes.extend(
        STANDARD_MODES
            .iter()
            .copied()
            .filter(|&(w, h)| w <= size.0 && h <= size.1 && (w, h) != size),
    );
    modes
}

/// ID of the virtual display, while there is one.
pub fn id() -> Option<u32> {
    CURRENT.lock().as_ref().map(|d| d.id)
}

/// Creates the virtual display (or returns the existing one) and waits until macOS
/// lists it.
pub fn create(size: (u32, u32)) -> Result<u32, String> {
    let mut current = CURRENT.lock();
    if let Some(d) = current.as_ref() {
        return Ok(d.id);
    }
    let display = build(size)?;
    let id = display.id;
    *current = Some(display);
    drop(current);
    let started = Instant::now();
    while !super::active_displays().contains(&id) {
        if started.elapsed() > Duration::from_secs(5) {
            remove();
            return Err(format!("virtual display {id} never came online"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    log::info!("created a {}x{} virtual display (id {id})", size.0, size.1);
    Ok(id)
}

/// Removes the virtual display, if any, and waits until macOS stops listing it.
pub fn remove() {
    let Some(display) = CURRENT.lock().take() else {
        return;
    };
    let id = display.id;
    drop(display);
    let started = Instant::now();
    while super::active_displays().contains(&id) && started.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(50));
    }
    log::info!("removed the virtual display (id {id})");
}

fn class(name: &CStr) -> Result<&'static AnyClass, String> {
    AnyClass::get(name).ok_or_else(|| {
        format!(
            "{} is missing: this macOS has no virtual display support",
            name.to_string_lossy()
        )
    })
}

fn new_object(class: &AnyClass) -> Retained<AnyObject> {
    unsafe {
        let allocated: Allocated<AnyObject> = msg_send![class, alloc];
        msg_send![allocated, init]
    }
}

fn build(size: (u32, u32)) -> Result<VirtualDisplay, String> {
    let descriptor_class = class(c"CGVirtualDisplayDescriptor")?;
    let display_class = class(c"CGVirtualDisplay")?;
    let settings_class = class(c"CGVirtualDisplaySettings")?;
    let mode_class = class(c"CGVirtualDisplayMode")?;
    let modes = modes_for(size);
    let (max_width, max_height) = modes
        .iter()
        .fold((0, 0), |(w, h), &(mw, mh)| (w.max(mw), h.max(mh)));
    let queue = DispatchQueue::new("voxl.virtual-display", None);
    unsafe {
        let descriptor = new_object(descriptor_class);
        let queue_object = &*queue as *const DispatchQueue as *const AnyObject;
        let _: () = msg_send![&*descriptor, setQueue: queue_object];
        let _: () = msg_send![&*descriptor, setName: &*NSString::from_str("voxl")];
        let _: () = msg_send![&*descriptor, setMaxPixelsWide: max_width];
        let _: () = msg_send![&*descriptor, setMaxPixelsHigh: max_height];
        let millimeters = CGSize {
            width: max_width as f64 / PIXELS_PER_MILLIMETER,
            height: max_height as f64 / PIXELS_PER_MILLIMETER,
        };
        let _: () = msg_send![&*descriptor, setSizeInMillimeters: millimeters];
        let _: () = msg_send![&*descriptor, setVendorID: 0x766fu32];
        let _: () = msg_send![&*descriptor, setProductID: 0x786cu32];
        let _: () = msg_send![&*descriptor, setSerialNum: 1u32];

        let allocated: Allocated<AnyObject> = msg_send![display_class, alloc];
        let display: Option<Retained<AnyObject>> =
            msg_send![allocated, initWithDescriptor: &*descriptor];
        let display = display.ok_or("CGVirtualDisplay refused the descriptor")?;

        let mode_objects: Vec<Retained<AnyObject>> = modes
            .iter()
            .map(|&(w, h)| {
                let allocated: Allocated<AnyObject> = msg_send![mode_class, alloc];
                msg_send![
                    allocated,
                    initWithWidth: w,
                    height: h,
                    refreshRate: REFRESH_RATE
                ]
            })
            .collect();
        let settings = new_object(settings_class);
        let _: () = msg_send![&*settings, setHiDPI: 0u32];
        let _: () = msg_send![&*settings, setModes: &*NSArray::from_retained_slice(&mode_objects)];
        let applied: Bool = msg_send![&*display, applySettings: &*settings];
        if !applied.as_bool() {
            return Err("CGVirtualDisplay refused the display modes".into());
        }
        let id: u32 = msg_send![&*display, displayID];
        if id == 0 {
            return Err("CGVirtualDisplay has no display ID".into());
        }
        Ok(VirtualDisplay {
            id,
            _display: display,
            _queue: queue,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("1920x1080"), Ok(Some((1920, 1080))));
        assert_eq!(parse_size(" 1281X721 "), Ok(Some((1280, 720))));
        assert_eq!(parse_size("off"), Ok(None));
        assert!(parse_size("1920").is_err());
        assert!(parse_size("100x100").is_err());
    }

    /// Adds a display to this Mac for a moment, so it only runs when asked for.
    #[test]
    #[ignore]
    fn creates_and_removes_a_display() {
        let id = create((1920, 1080)).unwrap();
        assert!(super::super::active_displays().contains(&id));
        let bounds = unsafe { super::super::ffi::CGDisplayBounds(id) };
        assert_eq!((bounds.size.width, bounds.size.height), (1920.0, 1080.0));
        remove();
        assert!(!super::super::active_displays().contains(&id));
    }

    #[test]
    fn requested_mode_comes_first() {
        let modes = modes_for((1920, 1080));
        assert_eq!(modes[0], (1920, 1080));
        assert_eq!(modes.iter().filter(|&&m| m == (1920, 1080)).count(), 1);
        assert!(modes.iter().all(|&(w, h)| w <= 1920 && h <= 1080));
        assert!(modes.contains(&(1280, 720)));
    }
}
