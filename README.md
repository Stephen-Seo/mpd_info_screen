# mpd info screen

[![mpd info screen crates.io version badge](https://img.shields.io/crates/v/mpd_info_screen)](https://crates.io/crates/mpd_info_screen)
[![mpd info screen license badge](https://img.shields.io/github/license/Stephen-Seo/mpd_info_screen)](https://choosealicense.com/licenses/mit/)

[Github Repository](https://github.com/Stephen-Seo/mpd_info_screen)

![mpd info screen preview image](https://git.seodisparate.com/stephenseo/mpd_info_screen/raw/branch/images/images/mpd_info_screen_preview_image.jpg)

A Rust program that displays info about the currently running MPD server.

The window shows albumart (may be embedded in the audio file, or is a "cover.jpg" in the same directory as the song file), a "time-remaining"
counter, and the filename currently being played

## Known Bugs ❗❗

Version `0.4.3` is a "workaround" release that is branched off of version
`0.3.7`. Once a new release of `ggez` is released that fixes the known bugs,
a new version will be released with the fixes. Because this is based on
`0.3.7` of `mpd_info_screen`, Wayland support may not work. Try using `xwayland`
with the environment variable `WINIT_UNIX_BACKEND=x11` set.

Currently, the dependency "ggez 0.8.1" [fails to render album
art](https://github.com/Stephen-Seo/mpd_info_screen/issues/1) on my machines
using the latest version of this program (`main` branch). Version 0.4.1 cannot
be published to https://crates.io due to this version referring to a git commit
as a dependency. Once ggez has released a new version with the commit that
fixes this bug, this repository will be updated to use that version.

The `devel` branch has a fix for mpd\_info\_screen not displaying properly when
no password is provided and MPD can be accessed without a password.

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


    mpd_info_screen 0.4.3
    
    USAGE:
        mpd_info_screen [FLAGS] [OPTIONS] <host> [port]
    
    FLAGS:
            --disable-show-album       disable album display
            --disable-show-artist      disable artist display
            --disable-show-filename    disable filename display
            --disable-show-title       disable title display
            --no-scale-fill            don't scale-fill the album art to the window
            --pprompt                  input password via prompt
        -h, --help                     Prints help information
        -V, --version                  Prints version information
    
    OPTIONS:
        -l, --log-level <log-level>                 [default: Error]  [possible values: Error, Warning, Debug, Verbose]
        -p <password>
        -t, --text-bg-opacity <text-bg-opacity>    sets the opacity of the text background (0-255) [default: 190]
    
    ARGS:
        <host>    
        <port>     [default: 6600]


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

Uses dependency [structopt](https://crates.io/crates/structopt) which is
licensed under Apache-2.0 or MIT licenses.

## Unicode Support Dependencies

Uses dependency
[fontconfig](https://www.freedesktop.org/wiki/Software/fontconfig/) which is
[licensed with this license](https://www.freedesktop.org/software/fontconfig/fontconfig-devel/ln12.html).

Uses dependency [freetype](https://freetype.org) which is
[licensed with this license](https://freetype.org/license.html).

Uses dependency [bindgen](https://crates.io/crates/bindgen) which is licenced
under the BSD-3-Clause.
