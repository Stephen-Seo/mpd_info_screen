use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

const SLEEP_DURATION: Duration = Duration::from_millis(100);
const POLL_DURATION: Duration = Duration::from_secs(10);
const BUF_SIZE: usize = 1024 * 4;

#[derive(Debug, PartialEq, Copy, Clone)]
enum PollState {
    None,
    Password,
    CurrentSong,
    Status,
    ReadPicture,
    ReadPictureInDir,
}

#[derive(Debug, Clone)]
pub struct InfoFromShared {
    filename: String,
    title: String,
    artist: String,
    length: f64,
    pos: f64,
    error_text: String,
}

pub struct MPDHandler {
    art_data: Vec<u8>,
    art_data_size: usize,
    current_song_filename: String,
    current_song_title: String,
    current_song_artist: String,
    current_song_length: f64,
    current_song_position: f64,
    current_binary_size: usize,
    poll_state: PollState,
    thread_running: bool,
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
    self_thread: Option<Arc<Mutex<thread::JoinHandle<Result<(), String>>>>>,
    dirty_flag: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
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
                    //println!("WARNING: OK was reached"); // DEBUG
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
            //println!("ERROR: {}", msg); // DEBUG
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

fn seconds_to_time(seconds: f64) -> String {
    let seconds_int: u64 = seconds.floor() as u64;
    let minutes = seconds_int / 60;
    let new_seconds: f64 = seconds - (minutes * 60) as f64;
    let mut result: String;
    if minutes > 0 {
        result = minutes.to_string();
        result.push(':');
        if new_seconds < 10.0 {
            result.push('0');
        }
    } else {
        result = String::new();
    }
    result.push_str(&new_seconds.to_string());
    let idx_result = result.find('.');
    if let Some(idx) = idx_result {
        result.truncate(idx + 2);
    }

    result
}

impl MPDHandler {
    pub fn new(host: Ipv4Addr, port: u16, password: String) -> Result<Arc<RwLock<Self>>, String> {
        let stream = TcpStream::connect_timeout(
            &SocketAddr::new(IpAddr::V4(host), port),
            Duration::from_secs(5),
        )
        .map_err(|_| String::from("Failed to get TCP connection"))?;

        let mut s = Arc::new(RwLock::new(Self {
            art_data: Vec::new(),
            art_data_size: 0,
            current_song_filename: String::new(),
            current_song_title: String::new(),
            current_song_artist: String::new(),
            current_song_length: 0.0,
            current_song_position: 0.0,
            current_binary_size: 0,
            poll_state: PollState::None,
            thread_running: true,
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
        }));

        let mut s_clone = s.clone();
        let mut thread = Arc::new(Mutex::new(thread::spawn(|| Self::handler_loop(s_clone))));

        s.write()
            .map_err(|_| String::from("Failed to start MPDHandler thread"))?
            .self_thread = Some(thread.clone());

        Ok(s)
    }

    pub fn get_art_data(h: Arc<RwLock<Self>>) -> Result<Vec<u8>, ()> {
        if let Ok(read_lock) = h.read() {
            if read_lock.art_data.len() == read_lock.art_data_size {
                return Ok(read_lock.art_data.clone());
            }
        }

        Err(())
    }

    pub fn can_get_art_data(h: Arc<RwLock<Self>>) -> bool {
        if let Ok(read_lock) = h.read() {
            return read_lock.can_get_album_art || read_lock.can_get_album_art_in_dir;
        }

        false
    }

    pub fn get_current_song_info(h: Arc<RwLock<Self>>) -> Result<InfoFromShared, ()> {
        if let Ok(read_lock) = h.read() {
            return Ok(InfoFromShared {
                filename: read_lock.current_song_filename.clone(),
                title: read_lock.current_song_title.clone(),
                artist: read_lock.current_song_artist.clone(),
                length: read_lock.current_song_length,
                pos: read_lock.current_song_position,
                error_text: read_lock.error_text.clone(),
            });
        }

        Err(())
    }

    fn handler_loop(h: Arc<RwLock<Self>>) -> Result<(), String> {
        let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
        let mut saved: Vec<u8> = Vec::new();
        let mut saved_str: String = String::new();
        'main: loop {
            if !Self::is_reading_picture(h.clone()) {
                thread::sleep(SLEEP_DURATION);
            }

            if let Err(err_string) =
                Self::handler_read_block(h.clone(), &mut buf, &mut saved, &mut saved_str)
            {
                println!("WARNING: read_block error: {}", err_string);
            } else if let Err(err_string) = Self::handler_write_block(h.clone()) {
                println!("WARNING: write_block error: {}", err_string);
            }

            if let Ok(read_handle) = h.read() {
                if read_handle.stop_flag.load(Ordering::Relaxed) {
                    break 'main;
                }
            }
        }

        'exit: loop {
            if let Ok(mut write_handle) = h.write() {
                write_handle.self_thread = None;
                break 'exit;
            }
            thread::sleep(SLEEP_DURATION);
        }

        Ok(())
    }

    fn handler_read_block(
        h: Arc<RwLock<Self>>,
        buf: &mut [u8; BUF_SIZE],
        saved: &mut Vec<u8>,
        saved_str: &mut String,
    ) -> Result<(), String> {
        let mut write_handle = h
            .write()
            .map_err(|_| String::from("Failed to get MPDHandler write lock (read_block)"))?;
        let read_result = write_handle
            .stream
            .read(buf)
            .map_err(|io_err| format!("Failed to read from TCP stream: {}", io_err))?;
        if read_result == 0 {
            return Err(String::from("Got zero bytes from TCP stream"));
        }
        let mut buf_vec: Vec<u8> = Vec::from(&buf[0..read_result]);

        'handle_buf: loop {
            if write_handle.current_binary_size > 0 {
                if write_handle.current_binary_size <= buf_vec.len() {
                    let count = write_handle.current_binary_size;
                    write_handle.art_data.extend_from_slice(&buf_vec[0..count]);
                    buf_vec = buf_vec.split_off(write_handle.current_binary_size + 1);
                    write_handle.current_binary_size = 0;
                    write_handle.poll_state = PollState::None;
                    write_handle.dirty_flag.store(true, Ordering::Relaxed);
                } else {
                    write_handle.art_data.extend_from_slice(&buf_vec);
                    write_handle.current_binary_size -= buf_vec.len();
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
                        println!("Got initial \"OK\" from MPD");
                        write_handle.poll_state = PollState::None;
                        break 'handle_buf;
                    } else {
                        return Err(String::from("Did not get expected init message from MPD"));
                    }
                } // write_handle.is_init

                if line.starts_with("OK") {
                    match write_handle.poll_state {
                        PollState::Password => write_handle.is_authenticated = true,
                        PollState::ReadPicture => {
                            if write_handle.art_data.is_empty() {
                                write_handle.can_get_album_art = false;
                                write_handle.dirty_flag.store(true, Ordering::Relaxed);
                                println!("No embedded album art");
                            }
                        }
                        PollState::ReadPictureInDir => {
                            if write_handle.art_data.is_empty() {
                                write_handle.can_get_album_art_in_dir = false;
                                write_handle.dirty_flag.store(true, Ordering::Relaxed);
                                println!("No album art in dir");
                            }
                        }
                        _ => (),
                    }
                    write_handle.poll_state = PollState::None;
                    break 'handle_buf;
                } else if line.starts_with("ACK") {
                    println!("ERROR: {}", line);
                    match write_handle.poll_state {
                        PollState::Password => {
                            write_handle.can_authenticate = false;
                            write_handle.dirty_flag.store(true, Ordering::Relaxed);
                        }
                        PollState::CurrentSong | PollState::Status => {
                            write_handle.can_get_status = false;
                            write_handle.dirty_flag.store(true, Ordering::Relaxed);
                        }
                        PollState::ReadPicture => {
                            write_handle.can_get_album_art = false;
                            write_handle.dirty_flag.store(true, Ordering::Relaxed);
                            println!("Failed to get readpicture");
                        }
                        PollState::ReadPictureInDir => {
                            write_handle.can_get_album_art_in_dir = false;
                            write_handle.dirty_flag.store(true, Ordering::Relaxed);
                            println!("Failed to get albumart");
                        }
                        _ => (),
                    }
                    write_handle.poll_state = PollState::None;
                } else if line.starts_with("file: ") {
                    let song_file = line.split_off(6);
                    if song_file != write_handle.current_song_filename {
                        write_handle.current_song_filename = song_file;
                        write_handle.art_data.clear();
                        write_handle.art_data_size = 0;
                        write_handle.can_get_album_art = true;
                        write_handle.can_get_album_art_in_dir = true;
                        write_handle.current_song_title.clear();
                        write_handle.current_song_artist.clear();
                        write_handle.current_song_length = 0.0;
                        write_handle.current_song_position = 0.0;
                        write_handle.did_check_overtime = false;
                        write_handle.force_get_status = true;
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
                        println!("WARNING: Failed to parse current song position");
                    }
                } else if line.starts_with("duration: ") {
                    let parse_pos_result = f64::from_str(&line.split_off(10));
                    if let Ok(value) = parse_pos_result {
                        write_handle.current_song_length = value;
                        write_handle.dirty_flag.store(true, Ordering::Relaxed);
                        write_handle.song_length_get_time = Instant::now();
                    } else {
                        println!("WARNING: Failed to parse current song duration");
                    }
                } else if line.starts_with("size: ") {
                    let parse_artsize_result = usize::from_str(&line.split_off(6));
                    if let Ok(value) = parse_artsize_result {
                        write_handle.art_data_size = value;
                        write_handle.dirty_flag.store(true, Ordering::Relaxed);
                    } else {
                        println!("WARNING: Failed to parse album art byte size");
                    }
                } else if line.starts_with("binary: ") {
                    let parse_artbinarysize_result = usize::from_str(&line.split_off(8));
                    if let Ok(value) = parse_artbinarysize_result {
                        write_handle.current_binary_size = value;
                    } else {
                        println!("WARNING: Failed to parse album art chunk byte size");
                    }
                } else if line.starts_with("Title: ") {
                    write_handle.current_song_title = line.split_off(7);
                } else if line.starts_with("Artist: ") {
                    write_handle.current_song_artist = line.split_off(8);
                } else {
                    println!("WARNING: Got unrecognized line: {}", line);
                }
            } else if let Err((msg, read_line_in_progress)) = read_line_result {
                println!("ERROR read_line: {}", msg);
                *saved_str = read_line_in_progress;
                break 'handle_buf;
            } else {
                unreachable!();
            }
        } // 'handle_buf: loop

        Ok(())
    }

    fn handler_write_block(h: Arc<RwLock<MPDHandler>>) -> Result<(), String> {
        let mut write_handle = h
            .write()
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
                    println!("ERROR: Failed to send password for authentication: {}", e);
                }
            } else if write_handle.can_get_status
                && (write_handle.song_title_get_time.elapsed() > POLL_DURATION
                    || write_handle.force_get_current_song)
            {
                write_handle.force_get_current_song = false;
                let write_result = write_handle.stream.write(b"currentsong\n");
                if write_result.is_ok() {
                    write_handle.poll_state = PollState::CurrentSong;
                } else if let Err(e) = write_result {
                    println!("ERROR: Failed to request song info over stream: {}", e);
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
                    println!("ERROR: Failed to request status over stream: {}", e);
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
                        println!("ERROR: Failed to request album art: {}", e);
                    }
                } else if write_handle.can_get_album_art_in_dir {
                    let write_result = write_handle
                        .stream
                        .write(format!("albumart \"{}\" {}\n", title, art_data_length).as_bytes());
                    if write_result.is_ok() {
                        write_handle.poll_state = PollState::ReadPictureInDir;
                    } else if let Err(e) = write_result {
                        println!("ERROR: Failed to request album art in dir: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    fn is_reading_picture(h: Arc<RwLock<MPDHandler>>) -> bool {
        if let Ok(read_handle) = h.read() {
            return read_handle.poll_state == PollState::ReadPicture
                || read_handle.poll_state == PollState::ReadPictureInDir;
        } else {
            return false;
        }
    }
}
