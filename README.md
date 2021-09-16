# mpd info screen

A Rust program that displays info about the currently running MPD server.

The window shows albumart (embedded in the audio file), a "time-remaining"
counter, and the filename currently being played

# Issues / TODO

[ ] UTF-8 Non-ascii font support
([macroquad](https://crates.io/crates/macroquad) is being used to display a
window, text, and album art, but doesn't seem to have support for ".ttc" fonts
that could render CJK text)
[ ] Support for album art not embedded but in the same directory

# Legal stuff

Uses dependency [macroquad](https://crates.io/crates/macroquad) which is
licensed under either MIT or Apache-2.0 licenses.

Uses dependency [image](https://crates.io/crates/image) which is licensed under
MIT license.

Uses dependency [structopt](https://crates.io/crates/structopt) which is
licensed under Apache-2.0 or MIT licenses.
