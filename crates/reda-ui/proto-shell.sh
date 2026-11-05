#!/bin/sh
# PROTOTYPE. Runs REDA on a scratch copy of a reference Project with a scratch REDA_HOME.
# REDA_SHELL=B or C starts on another preset; the floating bar switches at any time.
set -e
here=$(cd "$(dirname "$0")/../.." && pwd)
scratch=$(mktemp -d -t reda-proto-shell.XXXXXX)
cp -r "$here/tests/projects/usb_c_mcu" "$scratch/usb_c_mcu"
cd "$here"
REDA_HOME="$scratch/home" exec cargo run --release -p reda-app -- "$scratch/usb_c_mcu"
