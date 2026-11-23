#!/usr/bin/env bash
# Builds the benchmark for arm64 Android, pushes it and a stream to the device, runs it.
# Usage: ./run.sh <stream.h264> --size WxH [benchmark options]
# ANDROID_SERIAL picks the device when several are connected.
set -euo pipefail
cd "$(dirname "$0")"

stream=${1:?usage: ./run.sh <stream.h264> --size WxH [options]}
shift

sdk=${ANDROID_HOME:?set ANDROID_HOME to the Android SDK}
ndk=${ANDROID_NDK_HOME:-$(ls -d "$sdk"/ndk/* | sort -V | tail -1)}
host=$(ls "$ndk/toolchains/llvm/prebuilt" | head -1)
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ndk/toolchains/llvm/prebuilt/$host/bin/aarch64-linux-android28-clang"

cargo build --release --target aarch64-linux-android

remote=/data/local/tmp/voxl-decode
adb shell mkdir -p "$remote"
adb push -q target/aarch64-linux-android/release/voxl-android-decode "$remote/"
adb push -q "$stream" "$remote/stream.h264"
adb shell "$remote/voxl-android-decode" "$remote/stream.h264" --csv "$remote/frames.csv" "$@"
adb pull -q "$remote/frames.csv" "${stream%.h264}.native.csv"
