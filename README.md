# mpd info screen

![mpd info screen preview image](https://git.seodisparate.com/stephenseo/mpd_info_screen/raw/branch/images/images/mpd_info_screen_preview_image.jpg)

A Rust program that displays info about the currently running MPD server.

The window shows albumart (may be embedded in the audio file, or is a "cover.jpg" in the same directory as the song file), a "time-remaining"
counter, and the filename currently being played

# Usage

    mpd_info_screen 0.2.14
    
    USAGE:
        mpd_info_screen [FLAGS] [OPTIONS] <host> [port]
    
    FLAGS:
            --disable-show-artist      disable artist display
            --disable-show-filename    disable filename display
            --disable-show-title       disable title display
            --no-scale-fill            don't scale-fill the album art to the window
            --pprompt                  input password via prompt
        -h, --help                     Prints help information
        -V, --version                  Prints version information
    
    OPTIONS:
        -l, --log-level <log-level>     [default: Error]  [possible values: Error, Warning, Debug, Verbose]
        -p <password>
    
    ARGS:
        <host>
        <port>     [default: 6600]


Note that presing the Escape key when the window is focused closes the program.

Also note that pressing the H key while displaying text will hide the text.

# Issues / TODO

- [ ] UTF-8 Non-ascii font support  
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
