use crate::debug_log::{log, LogLevel, LogState};
use std::fmt::Write;
use std::io::{self, Read, Write as IOWrite};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard};
use std::thread;
use std::time::{Duration, Instant};

const SLEEP_DURATION: Duration = Duration::from_millis(100);
const POLL_DURATION: Duration = Duration::from_secs(5);
const BUF_SIZE: usize = 1024 * 4;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum PollState {
    None,
    Password,
    CurrentSong,
    Status,
    ReadPicture,
    ReadPictureInDir,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MPDPlayState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone)]
pub struct InfoFromShared {
    pub filename: String,
    pub title: String,
    pub artist: String,
    pub length: f64,
    pub pos: f64,
    pub error_text: String,
    pub mpd_play_state: MPDPlayState,
}

#[derive(Clone)]
pub struct MPDHandler {
    state: Arc<RwLock<MPDHandlerState>>,
}

type SelfThreadT = Option<Arc<Mutex<thread::JoinHandle<Result<(), String>>>>>;

pub struct MPDHandlerState {
    art_data: Vec<u8>,
    art_data_size: usize,
    art_data_type: String,
    current_song_filename: String,
    current_song_title: String,
    current_song_artist: String,
    current_song_length: f64,
    current_song_position: f64,
    current_binary_size: usize,
    poll_state: PollState,
    stream: TcpStream,
    password: String,
    error_text: String,
    can_authenticate: bool,
    is_authenticated: bool,
    can_get_album_art: bool,
    can_get_album_art_in_dir: bool,
    can_get_status: bool,
    is_init: bool,
    did_check_overtime: bool,
    force_get_status: bool,
    force_get_current_song: bool,
    song_title_get_time: Instant,
    song_pos_get_time: Instant,
    song_length_get_time: Instant,
    self_thread: SelfThreadT,
    dirty_flag: Arc<AtomicBool>,
    pub stop_flag: Arc<AtomicBool>,
    log_level: LogLevel,
    mpd_play_state: MPDPlayState,
}

fn check_next_chars(
    buf: &[u8],
    idx: usize,
    saved: &mut Vec<u8>,
) -> Result<(char, u8), (String, u8)> {
    if idx >= buf.len() {
        return Err((String::from("idx out of bounds"), 0u8));
    }
    if buf[idx] & 0b10000000 == 0 {
        let result_str = String::from_utf8(vec![buf[idx]]);
        if let Ok(mut s) = result_str {
            let popped_char = s.pop();
            if s.is_empty() {
                Ok((popped_char.unwrap(), 1u8))
            } else {
                Err((String::from("Not one-byte UTF-8 char"), 0u8))
            }
        } else {
            Err((String::from("Not one-byte UTF-8 char"), 0u8))
        }
    } else if buf[idx] & 0b11100000 == 0b11000000 {
        if idx + 1 >= buf.len() {
            saved.push(buf[idx]);
            return Err((
                String::from("Is two-byte UTF-8, but not enough bytes provided"),
                1u8,
            ));
        }
        let result_str = String::from_utf8(vec![buf[idx], buf[idx + 1]]);
        if let Ok(mut s) = result_str {
            let popped_char = s.pop();
            if s.is_empty() {
                Ok((popped_char.unwrap(), 2u8))
            } else {
                Err((String::from("Not two-byte UTF-8 char"), 0u8))
            }
        } else {
            Err((String::from("Not two-byte UTF-8 char"), 0u8))
        }
    } else if buf[idx] & 0b11110000 == 0b11100000 {
        if idx + 2 >= buf.len() {
            for c in buf.iter().skip(idx) {
                saved.push(*c);
            }
            return Err((
                String::from("Is three-byte UTF-8, but not enough bytes provided"),
                (idx + 3 - buf.len()) as u8,
            ));
        }
        let result_str = String::from_utf8(vec![buf[idx], buf[idx + 1], buf[idx + 2]]);
        if let Ok(mut s) = result_str {
            let popped_char = s.pop();
            if s.is_empty() {
                Ok((popped_char.unwrap(), 3u8))
            } else {
                Err((String::from("Not three-byte UTF-8 char"), 0u8))
            }
        } else {
            Err((String::from("Not three-byte UTF-8 char"), 0u8))
        }
    } else if buf[idx] & 0b11111000 == 0b11110000 {
        if idx + 3 >= buf.len() {
            for c in buf.iter().skip(idx) {
                saved.push(*c);
            }
            return Err((
                String::from("Is four-byte UTF-8, but not enough bytes provided"),
                (idx + 4 - buf.len()) as u8,
            ));
        }
        let result_str = String::from_utf8(vec![buf[idx], buf[idx + 1], buf[idx + 2]]);
        if let Ok(mut s) = result_str {
            let popped_char = s.pop();
            if s.is_empty() {
                Ok((popped_char.unwrap(), 4u8))
            } else {
                Err((String::from("Not four-byte UTF-8 char"), 0u8))
            }
        } else {
            Err((String::from("Not four-byte UTF-8 char"), 0u8))
        }
    } else {
        Err((String::from("Invalid UTF-8 char"), 0u8))
    }
}

fn read_line(
    buf: &mut Vec<u8>,
    saved: &mut Vec<u8>,
    init: bool,
) -> Result<String, (String, String)> {
    let count = buf.len();
    let mut result = String::new();

    if count == 0 {
        return Err((
            String::from("Empty string passed to read_line"),
            String::new(),
        ));
    }

    let mut buf_to_read: Vec<u8> = Vec::with_capacity(saved.len() + buf.len());

    if !saved.is_empty() {
        buf_to_read.append(saved);
    }
    buf_to_read.append(buf);

    let mut prev_three: Vec<char> = Vec::with_capacity(4);

    let mut skip_count = 0;
    for idx in 0..count {
        if skip_count > 0 {
            skip_count -= 1;
            continue;
        }
        let next_char_result = check_next_chars(&buf_to_read, idx, saved);
        if let Ok((c, s)) = next_char_result {
            if !init {
                prev_three.push(c);
                if prev_three.len() > 3 {
                    prev_three.remove(0);
                }
                if ['O', 'K', '\n'] == prev_three.as_slice() && idx + 1 == count {
                    buf_to_read = buf_to_read.split_off(2);
                    result = String::from("OK");
                    buf.append(&mut buf_to_read);
                    //println!("Warning: OK was reached"); // DEBUG
                    return Ok(result);
                }
            }
            if c == '\n' {
                buf_to_read = buf_to_read.split_off(idx + s as usize);
                buf.append(&mut buf_to_read);
                return Ok(result);
            }
            result.push(c);
            skip_count = s - 1;
        } else if let Err((msg, count)) = next_char_result {
            //println!("Error: {}", msg); // DEBUG
            for i in 0..count {
                saved.push(buf_to_read[idx + i as usize]);
            }
            buf_to_read = buf_to_read.split_off(idx);
            buf.append(&mut buf_to_read);
            return Err((msg, result));
        } else {
            unreachable!();
        }
    }

    *saved = buf_to_read;
    Err((String::from("Newline not reached"), result))
}

impl MPDHandler {
    pub fn new(
        host: Ipv4Addr,
        port: u16,
        password: String,
        log_level: LogLevel,
    ) -> Result<Self, String> {
        let stream = TcpStream::connect_timeout(
            &SocketAddr::new(IpAddr::V4(host), port),
            Duration::from_secs(5),
        )
        .map_err(|_| String::from("Failed to get TCP connection (is MPD running?)"))?;

        let s = MPDHandler {
            state: Arc::new(RwLock::new(MPDHandlerState {
                art_data: Vec::new(),
                art_data_size: 0,
                art_data_type: String::new(),
                current_song_filename: String::new(),
                current_song_title: String::new(),
                current_song_artist: String::new(),
                current_song_length: 0.0,
                current_song_position: 0.0,
                current_binary_size: 0,
                poll_state: PollState::None,
                stream,
                password,
                error_text: String::new(),
                can_authenticate: true,
                is_authenticated: false,
                can_get_album_art: true,
                can_get_album_art_in_dir: true,
                can_get_status: true,
                is_init: true,
                did_check_overtime: false,
                force_get_status: false,
                force_get_current_song: false,
                song_title_get_time: Instant::now() - Duration::from_secs(10),
                song_pos_get_time: Instant::now() - Duration::from_secs(10),
                song_length_get_time: Instant::now() - Duration::from_secs(10),
                self_thread: None,
                dirty_flag: Arc::new(AtomicBool::new(true)),
                stop_flag: Arc::new(AtomicBool::new(false)),
                log_level,
                mpd_play_state: MPDPlayState::Stopped,
            })),
        };

        let s_clone = s.clone();
        let thread = Arc::new(Mutex::new(thread::spawn(|| s_clone.handler_loop())));

        loop {
            if let Ok(mut write_handle) = s.state.try_write() {
                write_handle.self_thread = Some(thread);
                break;
            } else {
                thread::sleep(Duration::from_millis(1));
            }
        }

        Ok(s)
    }

    pub fn get_mpd_handler_shared_state(&self) -> Result<InfoFromShared, ()> {
        if let Ok(read_lock) = self.state.try_read() {
            return Ok(InfoFromShared {
                filename: read_lock.current_song_filename.clone(),
                title: read_lock.current_song_title.clone(),
                artist: read_lock.current_song_artist.clone(),
                length: read_lock.current_song_length,
                pos: read_lock.current_song_position
                    + read_lock.song_pos_get_time.elapsed().as_secs_f64(),
                error_text: read_lock.error_text.clone(),
                mpd_play_state: read_lock.mpd_play_state,
            });
        }

        Err(())
    }

    pub fn get_dirty_flag(&self) -> Result<Arc<AtomicBool>, ()> {
        if let Ok(read_lock) = self.state.try_read() {
            return Ok(read_lock.dirty_flag.clone());
        }

        Err(())
    }

    #[allow(dead_code)]
    pub fn is_dirty(&self) -> Result<bool, ()> {
        if let Ok(write_lock) = self.state.try_write() {
            return Ok(write_lock.dirty_flag.swap(false, Ordering::Relaxed));
        }

        Err(())
    }

    #[allow(dead_code)]
    pub fn force_get_current_song(&self) {
        loop {
            if let Ok(mut write_lock) = self.state.try_write() {
                write_lock.force_get_current_song = true;
                break;
            } else {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    pub fn is_authenticated(&self) -> Result<bool, ()> {
        let read_handle = self.state.try_read().map_err(|_| ())?;
        Ok(read_handle.is_authenticated)
    }

    pub fn failed_to_authenticate(&self) -> Result<bool, ()> {
        let read_handle = self.state.try_read().map_err(|_| ())?;
        Ok(!read_handle.can_authenticate)
    }

    #[allow(dead_code)]
    pub fn has_image_data(&self) -> Result<bool, ()> {
        let read_handle = self.state.try_read().map_err(|_| ())?;
        Ok(read_handle.is_art_data_ready())
    }

    pub fn get_state_read_guard(&self) -> Result<RwLockReadGuard<'_, MPDHandlerState>, ()> {
        self.state.try_read().map_err(|_| ())
    }

    pub fn stop_thread(&self) -> Result<(), ()> {
        let read_handle = self.state.try_read().map_err(|_| ())?;
        read_handle.stop_flag.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn force_try_other_album_art(&self) -> Result<(), ()> {
        let mut write_handle = self.state.try_write().map_err(|_| ())?;
        write_handle.art_data.clear();
        write_handle.art_data_size = 0;
        write_handle.can_get_album_art = false;
        write_handle.can_get_album_art_in_dir = true;
        Ok(())
    }

    fn handler_loop(self) -> Result<(), String> {
        let log_level = self
            .state
            .read()
            .expect("Failed to get log_level")
            .log_level;
        let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
        let mut saved: Vec<u8> = Vec::new();
        let mut saved_str: String = String::new();

        loop {
            if let Ok(write_handle) = self.state.try_write() {
                write_handle
                    .stream
                    .set_nonblocking(true)
                    .map_err(|_| String::from("Failed to set non-blocking on TCP stream"))?;
                break;
            } else {
                thread::sleep(POLL_DURATION);
            }
        }

        'main: loop {
            if !self.is_reading_picture()
                && self.is_authenticated().unwrap_or(true)
                && !self.failed_to_authenticate().unwrap_or(false)
            {
                thread::sleep(SLEEP_DURATION);
                if let Ok(write_handle) = self.state.try_write() {
                    if write_handle.self_thread.is_none() {
                        // main thread failed to store handle to this thread
                        log(
                            "MPDHandle thread stopping due to failed handle storage",
                            LogState::Error,
                            write_handle.log_level,
                        );
                        break 'main;
                    }
                }
            }

            if let Err(err_string) = self.handler_read_block(&mut buf, &mut saved, &mut saved_str) {
                log(
                    format!("read_block error: {}", err_string),
                    LogState::Warning,
                    log_level,
                );
            } else if let Err(err_string) = self.handler_write_block() {
                log(
                    format!("write_block error: {}", err_string),
                    LogState::Warning,
                    log_level,
                );
            }

            if let Ok(read_handle) = self.state.try_read() {
                if read_handle.stop_flag.load(Ordering::Relaxed) || !read_handle.can_authenticate {
                    break 'main;
                }
            }

            io::stdout().flush().unwrap();
        }

        log(
            "MPDHandler thread entering exit loop",
            LogState::Debug,
            log_level,
        );
        'exit: loop {
            if let Ok(mut write_handle) = self.state.try_write() {
                write_handle.self_thread = None;
                break 'exit;
            }
            thread::sleep(SLEEP_DURATION);
        }

        Ok(())
    }

    fn handler_read_block(
        &self,
        buf: &mut [u8; BUF_SIZE],
        saved: &mut Vec<u8>,
        saved_str: &mut String,
    ) -> Result<(), String> {
        let mut write_handle = self
            .state
            .try_write()
            .map_err(|_| String::from("Failed to get MPDHandler write lock (read_block)"))?;
        let mut read_amount: usize = 0;
        let read_result = write_handle.stream.read(buf);
        if let Err(io_err) = read_result {
            if io_err.kind() != io::ErrorKind::WouldBlock {
                return Err(format!("TCP stream error: {}", io_err));
            } else {
                return Ok(());
            }
        } else if let Ok(read_amount_result) = read_result {
            if read_amount_result == 0 {
                return Err(String::from("Got zero bytes from TCP stream"));
            }
            read_amount = read_amount_result;
        }
        let mut buf_vec: Vec<u8> = Vec::from(&buf[0..read_amount]);

        let mut got_mpd_state: MPDPlayState = MPDPlayState::Playing;

        'handle_buf: loop {
            if write_handle.current_binary_size > 0 {
                if write_handle.current_binary_size <= buf_vec.len() {
                    let count = write_handle.current_binary_size;
                    write_handle.art_data.extend_from_slice(&buf_vec[0..count]);
                    buf_vec = buf_vec.split_off(count + 1);
                    write_handle.current_binary_size = 0;
                    write_handle.poll_state = PollState::None;
                    log(
                        format!(
                            "Album art recv progress: {}/{}",
                            write_handle.art_data.len(),
                            write_handle.art_data_size
                        ),
                        LogState::Debug,
                        write_handle.log_level,
                    );
                    if write_handle.art_data.len() == write_handle.art_data_size {
                        write_handle.dirty_flag.store(true, Ordering::Relaxed);
                    }
                } else {
                    write_handle.art_data.extend_from_slice(&buf_vec);
                    write_handle.current_binary_size -= buf_vec.len();
                    log(
                        format!(
                            "Album art recv progress: {}/{}",
                            write_handle.art_data.len(),
                            write_handle.art_data_size
                        ),
                        LogState::Debug,
                        write_handle.log_level,
                    );
                    if write_handle.art_data.len() == write_handle.art_data_size {
                        write_handle.dirty_flag.store(true, Ordering::Relaxed);
                    }
                    break 'handle_buf;
                }
            }
            let read_line_result = read_line(&mut buf_vec, saved, write_handle.is_init);
            if let Ok(mut line) = read_line_result {
                line = saved_str.clone() + &line;
                *saved_str = String::new();
                if write_handle.is_init {
                    if line.starts_with("OK MPD ") {
                        write_handle.is_init = false;
                        log(
                            "Got initial \"OK\" from MPD",
                            LogState::Debug,
                            write_handle.log_level,
                        );
                        write_handle.poll_state = PollState::None;
                        break 'handle_buf;
                    } else {
                        return Err(String::from("Did not get expected init message from MPD"));
                    }
                } // write_handle.is_init

                if line.starts_with("OK") {
                    log(
                        format!("Got OK when poll state is {:?}", write_handle.poll_state),
                        LogState::Debug,
                        write_handle.log_level,
                    );
                    match write_handle.poll_state {
                        PollState::Password => write_handle.is_authenticated = true,
                        PollState::ReadPicture => {
                            if write_handle.art_data.is_empty() {
                                write_handle.can_get_album_art = false;
                                write_handle.dirty_flag.store(true, Ordering::Relaxed);
                                log(
                                    "No embedded album art",
                                    LogState::Warning,
                                    write_handle.log_level,
                                );
                            }
                        }
                        PollState::ReadPictureInDir => {
                            if write_handle.art_data.is_empty() {
                                write_handle.can_get_album_art_in_dir = false;
                                write_handle.dirty_flag.store(true, Ordering::Relaxed);
                                log(
                                    "No album art in dir",
                                    LogState::Warning,
                                    write_handle.log_level,
                                );
                            }
                        }
                        _ => (),
                    }
                    write_handle.poll_state = PollState::None;
                    break 'handle_buf;
                } else if line.starts_with("ACK") {
                    log(&line, LogState::Warning, write_handle.log_level);
                    match write_handle.poll_state {
                        PollState::Password => {
                            write_handle.can_authenticate = false;
                            write_handle.dirty_flag.store(true, Ordering::Relaxed);
                            write_handle.error_text = "Failed to authenticate to MPD".into();
                            write_handle.stop_flag.store(true, Ordering::Relaxed);
                        }
                        PollState::CurrentSong | PollState::Status => {
                            write_handle.can_get_status = false;
                            write_handle.dirty_flag.store(true, Ordering::Relaxed);
                            write_handle.error_text = "Failed to get MPD status".into();
                            if line.contains("don't have permission") {
                                write_handle.can_authenticate = false;
                                write_handle.error_text.push_str(" (not authenticated?)");
                            }
                        }
                        PollState::ReadPicture => {
                            write_handle.can_get_album_art = false;
                            write_handle.dirty_flag.store(true, Ordering::Relaxed);
                            log(
                                "Failed to get readpicture",
                                LogState::Warning,
                                write_handle.log_level,
                            );
                            // Not setting error_text here since
                            // ReadPictureInDir is tried next
                        }
                        PollState::ReadPictureInDir => {
                            write_handle.can_get_album_art_in_dir = false;
                            write_handle.dirty_flag.store(true, Ordering::Relaxed);
                            log(
                                "Failed to get albumart",
                                LogState::Warning,
                                write_handle.log_level,
                            );
                            write_handle.error_text = "Failed to get album art from MPD".into();
                        }
                        _ => (),
                    }
                    write_handle.poll_state = PollState::None;
                } else if line.starts_with("state: ") {
                    let remaining = line.split_off(7);
                    let remaining = remaining.trim();
                    if remaining == "stop" {
                        write_handle.current_song_filename.clear();
                        write_handle.art_data.clear();
                        write_handle.art_data_size = 0;
                        write_handle.art_data_type.clear();
                        write_handle.can_get_album_art = true;
                        write_handle.can_get_album_art_in_dir = true;
                        write_handle.current_song_title.clear();
                        write_handle.current_song_artist.clear();
                        write_handle.current_song_length = 0.0;
                        write_handle.current_song_position = 0.0;
                        write_handle.did_check_overtime = false;
                        write_handle.force_get_status = true;
                    }
                    if remaining == "stop" || remaining == "pause" {
                        got_mpd_state = if remaining == "stop" {
                            MPDPlayState::Stopped
                        } else {
                            MPDPlayState::Paused
                        };
                        write_handle.error_text.clear();
                        write!(&mut write_handle.error_text, "MPD has {:?}", got_mpd_state).ok();
                        log(
                            format!("MPD is {:?}", got_mpd_state),
                            LogState::Warning,
                            write_handle.log_level,
                        );
                        break 'handle_buf;
                    }
                } else if line.starts_with("file: ") {
                    let song_file = line.split_off(6);
                    if song_file != write_handle.current_song_filename {
                        write_handle.current_song_filename = song_file;
                        write_handle.art_data.clear();
                        write_handle.art_data_size = 0;
                        write_handle.art_data_type.clear();
                        write_handle.can_get_album_art = true;
                        write_handle.can_get_album_art_in_dir = true;
                        write_handle.current_song_title.clear();
                        write_handle.current_song_artist.clear();
                        write_handle.current_song_length = 0.0;
                        write_handle.current_song_position = 0.0;
                        write_handle.did_check_overtime = false;
                        write_handle.force_get_status = true;
                        write_handle.error_text.clear();
                    }
                    write_handle.dirty_flag.store(true, Ordering::Relaxed);
                    write_handle.song_title_get_time = Instant::now();
                } else if line.starts_with("elapsed: ") {
                    let parse_pos_result = f64::from_str(&line.split_off(9));
                    if let Ok(value) = parse_pos_result {
                        write_handle.current_song_position = value;
                        write_handle.dirty_flag.store(true, Ordering::Relaxed);
                        write_handle.song_pos_get_time = Instant::now();
                    } else {
                        log(
                            "Failed to parse current song position",
                            LogState::Warning,
                            write_handle.log_level,
                        );
                    }
                } else if line.starts_with("duration: ") {
                    let parse_pos_result = f64::from_str(&line.split_off(10));
                    if let Ok(value) = parse_pos_result {
                        write_handle.current_song_length = value;
                        write_handle.dirty_flag.store(true, Ordering::Relaxed);
                        write_handle.song_length_get_time = Instant::now();
                    } else {
                        log(
                            "Failed to parse current song duration",
                            LogState::Warning,
                            write_handle.log_level,
                        );
                    }
                } else if line.starts_with("size: ") {
                    let parse_artsize_result = usize::from_str(&line.split_off(6));
                    if let Ok(value) = parse_artsize_result {
                        write_handle.art_data_size = value;
                        write_handle.dirty_flag.store(true, Ordering::Relaxed);
                    } else {
                        log(
                            "Failed to parse album art byte size",
                            LogState::Warning,
                            write_handle.log_level,
                        );
                    }
                } else if line.starts_with("binary: ") {
                    let parse_artbinarysize_result = usize::from_str(&line.split_off(8));
                    if let Ok(value) = parse_artbinarysize_result {
                        write_handle.current_binary_size = value;
                    } else {
                        log(
                            "Failed to parse album art chunk byte size",
                            LogState::Warning,
                            write_handle.log_level,
                        );
                    }
                } else if line.starts_with("Title: ") {
                    write_handle.current_song_title = line.split_off(7);
                } else if line.starts_with("Artist: ") {
                    write_handle.current_song_artist = line.split_off(8);
                } else if line.starts_with("type: ") {
                    write_handle.art_data_type = line.split_off(6);
                } else {
                    log(
                        format!("Got unrecognized/ignored line: {}", line),
                        LogState::Warning,
                        write_handle.log_level,
                    );
                }
            } else if let Err((msg, read_line_in_progress)) = read_line_result {
                log(
                    format!(
                        "read_line: {}, saved size == {}, in_progress size == {}",
                        msg,
                        saved.len(),
                        read_line_in_progress.len()
                    ),
                    LogState::Warning,
                    write_handle.log_level,
                );
                *saved_str = read_line_in_progress;
                break 'handle_buf;
            } else {
                unreachable!();
            }
        } // 'handle_buf: loop

        if got_mpd_state != write_handle.mpd_play_state {
            write_handle.dirty_flag.store(true, Ordering::Relaxed);
            if got_mpd_state == MPDPlayState::Playing {
                write_handle.error_text.clear();
            }
        }
        write_handle.mpd_play_state = got_mpd_state;
        if got_mpd_state != MPDPlayState::Playing {
            write_handle.poll_state = PollState::None;
            write_handle.song_pos_get_time = Instant::now();
            write_handle.current_song_length = 30.0;
            write_handle.current_song_position = 0.0;
        }

        Ok(())
    }

    fn handler_write_block(&self) -> Result<(), String> {
        let mut write_handle = self
            .state
            .try_write()
            .map_err(|_| String::from("Failed to get MPDHandler write lock (write_block)"))?;
        if write_handle.poll_state == PollState::None {
            if !write_handle.did_check_overtime
                && write_handle.current_song_position
                    + write_handle.song_pos_get_time.elapsed().as_secs_f64()
                    - 0.2
                    > write_handle.current_song_length
            {
                write_handle.did_check_overtime = true;
                write_handle.force_get_current_song = true;
            }

            if !write_handle.is_authenticated
                && !write_handle.password.is_empty()
                && write_handle.can_authenticate
            {
                let p = write_handle.password.clone();
                let write_result = write_handle
                    .stream
                    .write(format!("password {}\n", p).as_bytes());
                if write_result.is_ok() {
                    write_handle.poll_state = PollState::Password;
                } else if let Err(e) = write_result {
                    log(
                        format!("Failed to send password for authentication: {}", e),
                        LogState::Error,
                        write_handle.log_level,
                    );
                }
            } else if write_handle.can_get_status
                && (write_handle.song_title_get_time.elapsed() > POLL_DURATION
                    || write_handle.force_get_current_song)
                && write_handle.mpd_play_state == MPDPlayState::Playing
            {
                write_handle.force_get_current_song = false;
                let write_result = write_handle.stream.write(b"currentsong\n");
                if write_result.is_ok() {
                    write_handle.poll_state = PollState::CurrentSong;
                } else if let Err(e) = write_result {
                    log(
                        format!("Failed to request song info over stream: {}", e),
                        LogState::Error,
                        write_handle.log_level,
                    );
                }
            } else if write_handle.can_get_status
                && (write_handle.song_length_get_time.elapsed() > POLL_DURATION
                    || write_handle.song_pos_get_time.elapsed() > POLL_DURATION
                    || write_handle.force_get_status)
            {
                write_handle.force_get_status = false;
                let write_result = write_handle.stream.write(b"status\n");
                if write_result.is_ok() {
                    write_handle.poll_state = PollState::Status;
                } else if let Err(e) = write_result {
                    log(
                        format!("Failed to request status over stream: {}", e),
                        LogState::Error,
                        write_handle.log_level,
                    );
                }
            } else if (write_handle.art_data.is_empty()
                || write_handle.art_data.len() != write_handle.art_data_size)
                && !write_handle.current_song_filename.is_empty()
            {
                let title = write_handle.current_song_filename.clone();
                let art_data_length = write_handle.art_data.len();
                if write_handle.can_get_album_art {
                    let write_result = write_handle.stream.write(
                        format!("readpicture \"{}\" {}\n", title, art_data_length).as_bytes(),
                    );
                    if write_result.is_ok() {
                        write_handle.poll_state = PollState::ReadPicture;
                    } else if let Err(e) = write_result {
                        log(
                            format!("Failed to request album art: {}", e),
                            LogState::Error,
                            write_handle.log_level,
                        );
                    }
                } else if write_handle.can_get_album_art_in_dir {
                    let write_result = write_handle
                        .stream
                        .write(format!("albumart \"{}\" {}\n", title, art_data_length).as_bytes());
                    if write_result.is_ok() {
                        write_handle.poll_state = PollState::ReadPictureInDir;
                    } else if let Err(e) = write_result {
                        log(
                            format!("Failed to request album art in dir: {}", e),
                            LogState::Error,
                            write_handle.log_level,
                        );
                    }
                }
            }
        }

        Ok(())
    }

    fn is_reading_picture(&self) -> bool {
        loop {
            if let Ok(read_handle) = self.state.try_read() {
                return read_handle.poll_state == PollState::ReadPicture
                    || read_handle.poll_state == PollState::ReadPictureInDir;
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

impl MPDHandlerState {
    pub fn get_art_type(&self) -> String {
        self.art_data_type.clone()
    }

    pub fn is_art_data_ready(&self) -> bool {
        log(
            format!(
                "is_art_data_ready(): art_data_size == {}, art_data.len() == {}",
                self.art_data_size,
                self.art_data.len()
            ),
            LogState::Debug,
            self.log_level,
        );
        self.art_data_size != 0 && self.art_data.len() == self.art_data_size
    }

    pub fn get_art_data(&self) -> &[u8] {
        &self.art_data
    }
}
