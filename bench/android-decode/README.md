# Android decode latency

Times H.264 decoding through the NDK's `AMediaCodec` on an Android device, against
Chrome's WebCodecs on the very same frames. It answers whether a native Android
client would decode the voxl stream faster than the browser does.

The same Rust code runs two ways:

- `run.sh`: a plain arm64 executable started from `adb shell`. Quick, but a shell
  process is never the foreground app, so vendor performance hints that follow the
  top app (MIUI's, MediaTek's) don't apply and decoding looks slower than it is.
- `apk.sh`: a NativeActivity APK (no Java, no Gradle) that runs the benchmark while it
  is the foreground app and can decode straight onto its own window, as a real
  native client would. Use this one for conclusions.

Frames are fed at a live stream's pace; each is timed from queueing its input to the
decoder returning it, which is what Chrome's numbers measure too.

| Output (`--buffer`, default, `--window`) | Where frames go |
|---|---|
| `Buffer` | byte buffers the decoder copies into |
| `ImageReader` | an offscreen `AImageReader` Surface (also timed to arrival there) |
| `Window` | the APK's window, composited like a video player's (APK only) |

## Requirements

- `rustup target add aarch64-linux-android`
- `ANDROID_HOME` pointing at an SDK with an NDK and build-tools (`ANDROID_NDK_HOME` to
  pick an NDK), and `~/.android/debug.keystore` (`DEBUG_KEYSTORE` to override)
- `adb` with one device (or `ANDROID_SERIAL`), and `bun` for the capture script

## Recording the stream Chrome decodes

Open voxl in Chrome on the phone, keep it in front with the screen on, then:

```sh
adb forward tcp:9333 localabstract:chrome_devtools_remote
curl -s localhost:9333/json/list    # the voxl tab's "id"
bun capture.ts <id> 20 voxl.h264
```

This reloads the tab, records 20 s of the H.264 its worker decodes into `voxl.h264`
(one access unit delimiter per frame), and Chrome's latency for each of those frames
into `voxl.chrome.csv`.

## Running

Send Chrome to the background first (Home), or it keeps decoding the live stream on
the same hardware decoder. For the APK, keep the phone unlocked with the screen on.

```sh
./apk.sh voxl.h264 --size 2560x1440 --fps 20 --window
./apk.sh voxl.h264 --size 2560x1440 --fps 20 --window --set operating-rate=240 --set priority=0
./run.sh voxl.h264 --size 1280x720 --codec c2.android.avc.decoder
```

`--fps` should match the recording (frames in the CSV / seconds). Per-frame numbers
land in `voxl.apk.csv` / `voxl.native.csv`. `--help` lists every option. Other streams
work if they carry access unit delimiters, e.g.
`ffmpeg ... -c:v libx264 -bf 0 -x264-params aud=1:repeat-headers=1 -f h264 out.h264`.

The APK reads its arguments from `/sdcard/Android/data/dev.voxl.decodebench/files/args.txt`
and writes `result.txt` there; `adb logcat -s voxl-decode` shows the same summary.

## Results: Redmi Note 10 5G (Dimensity 700, `c2.mtk.avc.decoder`), Android 13, Chrome 158

400 frames of a real 2560x1440 voxl stream at 20 fps, decode latency in ms:

| | p10 | p50 | p90 | p99 |
|---|---|---|---|---|
| Chrome WebCodecs (foreground tab) | 16.0 | 17.9 | 28.5 | 49.5 |
| APK, `Window` | 13.0 | 14.5 | 15.8 | 17.8 |
| APK, `Window`, `operating-rate=240 priority=0` | 9.9 | 13.2 | 15.3 | 16.5 |
| APK, `Window`, same content scaled to 1280x720 | 8.2 | 10.4 | 11.5 | 12.7 |
| APK, `ImageReader` | 26.3 | 31.8 | 33.3 | 35.2 |
| APK, `Buffer` | 32.2 | 36.8 | 39.0 | 41.4 |
| shell, `ImageReader` | 31.7 | 33.0 | 33.9 | 36.2 |

Decoding onto a window is what makes MediaTek's decoder fast. The likely reason is
that frames stay in the decoder's own layout for the display to scan out, while an
`ImageReader` or byte buffers make it convert every frame. `KEY_LOW_LATENCY` changed
nothing on this device.
