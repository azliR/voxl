#!/usr/bin/env bash
# Builds the NativeActivity APK, installs it, runs one benchmark in the foreground and
# prints the result. Same arguments as run.sh.
# Usage: ./apk.sh <stream.h264> --size WxH [benchmark options, e.g. --window]
# ANDROID_SERIAL picks the device when several are connected.
set -euo pipefail
cd "$(dirname "$0")"

stream=${1:?usage: ./apk.sh <stream.h264> --size WxH [options]}
shift

sdk=${ANDROID_HOME:?set ANDROID_HOME to the Android SDK}
ndk=${ANDROID_NDK_HOME:-$(ls -d "$sdk"/ndk/* | sort -V | tail -1)}
host=$(ls "$ndk/toolchains/llvm/prebuilt" | head -1)
build_tools=$(ls -d "$sdk"/build-tools/* | sort -V | tail -1)
platform=$(ls -d "$sdk"/platforms/android-* | sort -V | tail -1)
keystore=${DEBUG_KEYSTORE:-$HOME/.android/debug.keystore}
package=dev.voxl.decodebench
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ndk/toolchains/llvm/prebuilt/$host/bin/aarch64-linux-android28-clang"

cargo build --release --target aarch64-linux-android --lib

out=target/apk
rm -rf "$out"
mkdir -p "$out/lib/arm64-v8a"
cp target/aarch64-linux-android/release/libvoxl_android_decode.so "$out/lib/arm64-v8a/"
"$build_tools/aapt2" link -o "$out/unsigned.apk" --manifest android/AndroidManifest.xml \
	-I "$platform/android.jar" --min-sdk-version 28 --target-sdk-version 34
(cd "$out" && zip -q -r unsigned.apk lib)
"$build_tools/zipalign" -f -P 16 4 "$out/unsigned.apk" "$out/aligned.apk"
"$build_tools/apksigner" sign --ks "$keystore" --ks-pass pass:android \
	--out "$out/voxl-decode-bench.apk" "$out/aligned.apk"

adb install -r -g "$out/voxl-decode-bench.apk" >/dev/null
remote=/sdcard/Android/data/$package/files
adb shell mkdir -p "$remote"
adb push -q "$stream" "$remote/stream.h264"
echo "stream.h264 --csv frames.csv $*" | adb shell "cat > $remote/args.txt"
adb shell rm -f "$remote/result.txt"
adb shell am force-stop "$package"
adb shell am start -n "$package/android.app.NativeActivity" >/dev/null

for _ in $(seq 1 120); do
	if adb shell test -f "$remote/result.txt"; then
		adb shell cat "$remote/result.txt"
		echo
		adb pull -q "$remote/frames.csv" "${stream%.h264}.apk.csv" 2>/dev/null || true
		exit 0
	fi
	sleep 1
done
echo "no result after 120 s; see adb logcat -s voxl-decode" >&2
exit 1
