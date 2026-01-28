# mpd info screen

[![mpd info screen crates.io version badge](https://img.shields.io/crates/v/mpd_info_screen)](https://crates.io/crates/mpd_info_screen)
[![mpd info screen license badge](https://img.shields.io/github/license/Stephen-Seo/mpd_info_screen)](https://choosealicense.com/licenses/mit/)

[Github Repository](https://github.com/Stephen-Seo/mpd_info_screen)

![mpd info screen preview image](https://github.com/Stephen-Seo/mpd_info_screen/blob/images/images/mpd_info_screen_preview_image.jpg?raw=true)

A Rust program that displays info about the currently running MPD server.

The window shows albumart (may be embedded in the audio file, or is a "cover.jpg" in the same directory as the song file), a "time-remaining"
counter, and the filename currently being played

## mpd\_info\_screen2

mpd\_info\_screen has been rewritten in C++ using Raylib instead of ggez. You can find it here:

[github](https://github.com/Stephen-Seo/mpd_info_screen2)

[git.seodisparate.com](https://git.seodisparate.com/gitweb/?p=mpd_info_screen2;a=summary)

For now, both programs will be maintained.

## Known Bugs ❗❗

Currently there are no known bugs. Please report any bugs you find to the
[issue tracker](https://github.com/Stephen-Seo/mpd_info_screen/issues).

## Unicode Support

By default, unicode characters will not display properly. Build the project with
the `unicode_support` feature enabled to enable fetching fonts from the local
filesystem to display unicode characters properly (if the system is missing a
font, then it will still be displayed incorrectly). Note that your system must
have `fontconfig` and `freetype` installed (most Linux systems should have these
installed already).

    cargo build --release --features unicode_support

or through crates.io:

    cargo install --features unicode_support mpd_info_screen

# Usage


    Displays info on currently playing music from an MPD daemon
    
    Usage: mpd_info_screen [OPTIONS] <HOST> [PORT]
    
    Arguments:
      <HOST>
      [PORT]  [default: 6600]
    
    Options:
      -p <PASSWORD>
    
          --disable-show-title
              disable title display
          --disable-show-artist
              disable artist display
          --disable-show-album
              disable album display
          --disable-show-filename
              disable filename display
          --disable-show-percentage
              disable percentage display
          --force-text-height-scale <FORCE_TEXT_HEIGHT_SCALE>
              force-set text height relative to window height as a ratio (default 0.12)
          --pprompt
              input password via prompt
          --pfile <PASSWORD_FILE>
              read password from file
          --no-scale-fill
              don't scale-fill the album art to the window
      -l, --log-level <LOG_LEVEL>
              [default: error] [possible values: error, warning, debug, verbose]
      -t, --text-bg-opacity <TEXT_BG_OPACITY>
              sets the opacity of the text background (0-255) [default: 190]
      -h, --help
              Print help
      -V, --version
              Print version


Note that presing the Escape key when the window is focused closes the program.

Also note that pressing the H key while displaying text will hide the text.

# Issues / TODO

- [x] UTF-8 Non-ascii font support (Use the `unicode_support` feature to enable; only tested in linux)
- [x] Support for album art not embedded but in the same directory

## MPD Version

To get album art from the image embedded with the audio file, the "readpicture"
protocol command is queried from MPD, which was added in version 0.22 of MPD.
It is uncertain when the "albumart" protocol command was added (this command
fetches album art that resides in cover.jpg/cover.png in the same directory as
the audio file). This means that older versions of MPD may not return album art
to display.

# Legal stuff

Uses dependency [ggez](https://github.com/ggez/ggez) which is licensed under the
MIT license.

Uses dependency [image](https://crates.io/crates/image) which is licensed under
MIT license.

Uses dependency [clap](https://crates.io/crates/clap) which is licensed under
Apache-2.0 or MIT licenses.

## Unicode Support Dependencies

Uses dependency
[fontconfig](https://www.freedesktop.org/wiki/Software/fontconfig/) which is
[licensed with this license](https://www.freedesktop.org/software/fontconfig/fontconfig-devel/ln12.html).

Uses dependency [freetype](https://freetype.org) which is
[licensed with this license](https://freetype.org/license.html).

Uses dependency [bindgen](https://crates.io/crates/bindgen) which is licenced
under the BSD-3-Clause.
