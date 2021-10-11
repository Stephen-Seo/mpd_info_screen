use image::{DynamicImage, ImageResult};
use macroquad::prelude::*;
use std::convert::TryInto;
//use std::fs::File;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::str::FromStr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};
use structopt::StructOpt;

const BUF_SIZE: usize = 1024 * 4;
const POLL_DURATION: Duration = Duration::from_secs(10);
const TEXT_X_OFFSET: f32 = 16.0f32;
const TEXT_Y_OFFSET: f32 = 16.0f32;
const TIME_MAX_DIFF: f64 = 2.0f64;
const INITIAL_FONT_SIZE: u16 = 96;
const TITLE_INITIAL_FONT_SIZE: u16 = 196;
const TITLE_INITIAL_MIN_FONT_SIZE: u16 = 44;
const TITLE_SCREEN_FACTOR: u16 = 800;
const ARTIST_INITIAL_FONT_SIZE: u16 = 48;
const TIMER_FONT_SIZE: u16 = 64;
const SCREEN_DIFF_MARGIN: f32 = 1.0;
const PROMPT_Y_OFFSET: f32 = 48.0;
const CHECK_SHARED_WAIT_TIME: f64 = 3.0;
const CHECK_TRACK_TIMER_MAX_COUNT: u64 = 30;

#[derive(StructOpt, Debug)]
#[structopt(name = "mpd_info_screen")]
struct Opt {
    host: Ipv4Addr,
    #[structopt(default_value = "6600")]
    port: u16,
    #[structopt(short = "p")]
    password: Option<String>,
    #[structopt(long = "disable-show-title", help = "disable title display")]
    disable_show_title: bool,
    #[structopt(long = "disable-show-artist", help = "disable artist display")]
    disable_show_artist: bool,
    #[structopt(long = "disable-show-filename", help = "disable filename display")]
    disable_show_filename: bool,
    #[structopt(long = "pprompt", help = "input password via prompt")]
    enable_prompt_password: bool,
}

struct Shared {
    art_data: Vec<u8>,
    art_data_size: usize,
    current_song_filename: String,
    current_song_title: String,
    current_song_artist: String,
    current_song_length: f64,
    current_song_position: f64,
    current_song_pos_rec: Instant,
    thread_running: bool,
    stream: TcpStream,
    password: String,
    can_authenticate: bool,
    can_get_album_art: bool,
    can_get_album_art_in_dir: bool,
    can_get_status: bool,
}

impl Shared {
    fn new(stream: TcpStream) -> Self {
        Self {
            art_data: Vec::new(),
            art_data_size: 0,
            current_song_filename: String::new(),
            current_song_title: String::new(),
            current_song_artist: String::new(),
            current_song_length: 0.0,
            current_song_position: 0.0,
            current_song_pos_rec: Instant::now(),
            thread_running: true,
            stream,
            password: String::new(),
            can_authenticate: true,
            can_get_album_art: true,
            can_get_album_art_in_dir: true,
            can_get_status: true,
        }
    }
}

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
struct InfoFromShared {
    filename: String,
    title: String,
    artist: String,
    length: f64,
    pos: f64,
    instant_rec: Instant,
    error_text: String,
}

fn get_connection(host: Ipv4Addr, port: u16) -> Result<TcpStream, String> {
    let stream = TcpStream::connect_timeout(
        &SocketAddr::new(IpAddr::V4(host), port),
        Duration::from_secs(5),
    )
    .map_err(|_| String::from("Failed to get connection"))?;

    Ok(stream)
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
    count: usize,
    saved: &mut Vec<u8>,
    init: bool,
) -> Result<String, (String, String)> {
    let mut result = String::new();

    let mut buf_to_read: Vec<u8> = Vec::with_capacity(saved.len() + buf.len());

    if !saved.is_empty() {
        buf_to_read.append(saved);
    }
    buf_to_read.append(buf);

    let mut prev_two: Vec<char> = Vec::with_capacity(3);

    let mut skip_count = 0;
    for idx in 0..count {
        if skip_count > 0 {
            skip_count -= 1;
            continue;
        }
        let next_char_result = check_next_chars(&buf_to_read, idx, saved);
        if let Ok((c, s)) = next_char_result {
            if !init {
                prev_two.push(c);
                if prev_two.len() > 2 {
                    prev_two.remove(0);
                }
                if ['O', 'K'] == prev_two.as_slice() {
                    buf_to_read = buf_to_read.split_off(2);
                    result = String::from("OK");
                    buf.append(&mut buf_to_read);
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

//fn debug_write_albumart_to_file(data: &[u8]) -> Result<(), String> {
//    let mut f = File::create("albumartOut.jpg")
//        .map_err(|_| String::from("Failed to open file for writing albumart"))?;
//    f.write_all(data)
//        .map_err(|_| String::from("Failed to write to albumartOut.jpg"))?;
//
//    Ok(())
//}

fn info_loop(shared_data: Arc<Mutex<Shared>>, dirty_flag: Arc<AtomicBool>) -> Result<(), String> {
    let mut buf: [u8; BUF_SIZE] = [0; BUF_SIZE];
    let mut init: bool = true;
    let mut saved: Vec<u8> = Vec::new();
    let mut saved_str: String = String::new();
    let mut authenticated: bool = false;
    let mut song_title_get_time: Instant = Instant::now() - Duration::from_secs(10);
    let mut song_pos_get_time: Instant = Instant::now() - Duration::from_secs(10);
    let mut song_length_get_time: Instant = Instant::now() - Duration::from_secs(10);
    let mut current_binary_size: usize = 0;
    let mut poll_state = PollState::None;
    let mut did_check_overtime = false;
    let mut force_get_currentsong = false;
    let mut force_get_status = false;
    'main: loop {
        if !shared_data
            .lock()
            .map_err(|_| String::from("Failed to get shared_data.thread_running"))?
            .thread_running
        {
            break;
        }

        // read block
        {
            let lock_result = shared_data.try_lock();
            if let Ok(mut lock) = lock_result {
                let read_result = lock.stream.read(&mut buf);
                if let Ok(count) = read_result {
                    let mut read_vec: Vec<u8> = Vec::from(&buf[0..count]);
                    loop {
                        let mut count = read_vec.len();
                        if current_binary_size > 0 {
                            if current_binary_size <= count {
                                lock.art_data
                                    .extend_from_slice(&read_vec[0..current_binary_size]);
                                read_vec = read_vec.split_off(current_binary_size + 1);
                                count = read_vec.len();
                                current_binary_size = 0;
                                poll_state = PollState::None;
                                dirty_flag.store(true, Ordering::Relaxed);
                                //println!(
                                //    "art_data len is {} after fully reading",
                                //    lock.art_data.len()
                                //);
                                // DEBUG
                                //let write_file_result = debug_write_albumart_to_file(&lock.art_data);
                            } else {
                                lock.art_data.extend_from_slice(&read_vec[0..count]);
                                current_binary_size -= count;
                                //println!("art_data len is {}", lock.art_data.len());
                                continue 'main;
                            }
                        }
                        let read_line_result = read_line(&mut read_vec, count, &mut saved, init);
                        if let Ok(mut line) = read_line_result {
                            line = saved_str + &line;
                            saved_str = String::new();
                            if init {
                                if line.starts_with("OK MPD ") {
                                    init = false;
                                    println!("Got initial \"OK\" from MPD");
                                    poll_state = PollState::None;
                                    break;
                                } else {
                                    return Err(String::from(
                                        "Did not get expected init message from MPD",
                                    ));
                                }
                            } else {
                                //println!("Got response: {}", line); // DEBUG
                                if line.starts_with("OK") {
                                    match poll_state {
                                        PollState::Password => authenticated = true,
                                        PollState::ReadPicture => {
                                            if lock.art_data.is_empty() {
                                                lock.can_get_album_art = false;
                                                dirty_flag.store(true, Ordering::Relaxed);
                                                println!("No embedded album art");
                                            }
                                        }
                                        PollState::ReadPictureInDir => {
                                            if lock.art_data.is_empty() {
                                                lock.can_get_album_art_in_dir = false;
                                                dirty_flag.store(true, Ordering::Relaxed);
                                                println!("No album art in dir");
                                            }
                                        }
                                        _ => (),
                                    }
                                    poll_state = PollState::None;
                                    break;
                                } else if line.starts_with("ACK") {
                                    println!("ERROR: {}", line);
                                    match poll_state {
                                        PollState::Password => {
                                            lock.can_authenticate = false;
                                            dirty_flag.store(true, Ordering::Relaxed);
                                        }
                                        PollState::CurrentSong | PollState::Status => {
                                            lock.can_get_status = false;
                                            dirty_flag.store(true, Ordering::Relaxed);
                                        }
                                        PollState::ReadPicture => {
                                            lock.can_get_album_art = false;
                                            dirty_flag.store(true, Ordering::Relaxed);
                                            println!("Failed to get readpicture");
                                        }
                                        PollState::ReadPictureInDir => {
                                            lock.can_get_album_art_in_dir = false;
                                            dirty_flag.store(true, Ordering::Relaxed);
                                            println!("Failed to get albumart");
                                        }
                                        _ => (),
                                    }
                                    poll_state = PollState::None;
                                } else if line.starts_with("file: ") {
                                    let song_file = line.split_off(6);
                                    if song_file != lock.current_song_filename {
                                        lock.current_song_filename = song_file;
                                        lock.art_data.clear();
                                        lock.art_data_size = 0;
                                        lock.can_get_album_art = true;
                                        lock.can_get_album_art_in_dir = true;
                                        //println!("Got different song file, clearing art_data...");
                                        lock.current_song_title.clear();
                                        lock.current_song_artist.clear();
                                        lock.current_song_length = 0.0;
                                        lock.current_song_position = 0.0;
                                        did_check_overtime = false;
                                        force_get_status = true;
                                    }
                                    dirty_flag.store(true, Ordering::Relaxed);
                                    song_title_get_time = Instant::now();
                                } else if line.starts_with("elapsed: ") {
                                    let parse_pos_result = f64::from_str(&line.split_off(9));
                                    if let Ok(value) = parse_pos_result {
                                        lock.current_song_position = value;
                                        dirty_flag.store(true, Ordering::Relaxed);
                                        song_pos_get_time = Instant::now();
                                        lock.current_song_pos_rec = Instant::now();
                                    } else {
                                        println!("Got error trying to get current_song_position");
                                    }
                                } else if line.starts_with("duration: ") {
                                    let parse_pos_result = f64::from_str(&line.split_off(10));
                                    if let Ok(value) = parse_pos_result {
                                        lock.current_song_length = value;
                                        dirty_flag.store(true, Ordering::Relaxed);
                                        song_length_get_time = Instant::now();
                                    }
                                } else if line.starts_with("size: ") {
                                    let parse_artsize_result = usize::from_str(&line.split_off(6));
                                    if let Ok(value) = parse_artsize_result {
                                        lock.art_data_size = value;
                                        dirty_flag.store(true, Ordering::Relaxed);
                                    }
                                } else if line.starts_with("binary: ") {
                                    let parse_artbinarysize_result =
                                        usize::from_str(&line.split_off(8));
                                    if let Ok(value) = parse_artbinarysize_result {
                                        current_binary_size = value;
                                    }
                                } else if line.starts_with("Title: ") {
                                    lock.current_song_title = line.split_off(7);
                                } else if line.starts_with("Artist: ") {
                                    lock.current_song_artist = line.split_off(8);
                                }
                            }
                        } else if let Err((msg, read_line_in_progress)) = read_line_result {
                            println!("Error during \"read_line\": {}", msg);
                            saved_str = read_line_in_progress;
                            break;
                        } else {
                            unreachable!();
                        }
                    }
                }
            } else {
                println!("INFO: Temporarily failed to acquire lock for reading from tcp stream");
            }
        }

        // write block
        if poll_state == PollState::None {
            let lock_result = shared_data.try_lock();
            if let Ok(mut lock) = lock_result {
                // first check if overtime
                if !did_check_overtime
                    && lock.current_song_position + song_pos_get_time.elapsed().as_secs_f64() - 0.2
                        > lock.current_song_length
                {
                    did_check_overtime = true;
                    force_get_currentsong = true;
                    //println!("set \"force_get_currentsong\""); // DEBUG
                }

                if !authenticated && !lock.password.is_empty() && lock.can_authenticate {
                    let p = lock.password.clone();
                    let write_result = lock.stream.write(format!("password {}\n", p).as_bytes());
                    if write_result.is_ok() {
                        poll_state = PollState::Password;
                    } else if let Err(e) = write_result {
                        println!("Got error requesting authentication: {}", e);
                    }
                } else if (song_title_get_time.elapsed() > POLL_DURATION || force_get_currentsong)
                    && lock.can_get_status
                {
                    force_get_currentsong = false;
                    let write_result = lock.stream.write(b"currentsong\n");
                    if let Err(e) = write_result {
                        println!("Got error requesting currentsong info: {}", e);
                    } else {
                        poll_state = PollState::CurrentSong;
                    }
                } else if (song_length_get_time.elapsed() > POLL_DURATION
                    || song_pos_get_time.elapsed() > POLL_DURATION
                    || force_get_status)
                    && lock.can_get_status
                {
                    force_get_status = false;
                    let write_result = lock.stream.write(b"status\n");
                    if let Err(e) = write_result {
                        println!("Got error requesting status: {}", e);
                    } else {
                        poll_state = PollState::Status;
                    }
                } else if (lock.art_data.is_empty() || lock.art_data.len() != lock.art_data_size)
                    && !lock.current_song_filename.is_empty()
                {
                    let title = lock.current_song_filename.clone();
                    let art_data_length = lock.art_data.len();
                    if lock.can_get_album_art {
                        let write_result = lock.stream.write(
                            format!("readpicture \"{}\" {}\n", title, art_data_length).as_bytes(),
                        );
                        if let Err(e) = write_result {
                            println!("Got error requesting albumart: {}", e);
                        } else {
                            poll_state = PollState::ReadPicture;
                            //println!("polling readpicture");
                        }
                    } else if lock.can_get_album_art_in_dir {
                        let write_result = lock.stream.write(
                            format!("albumart \"{}\" {}\n", title, art_data_length).as_bytes(),
                        );
                        if let Err(e) = write_result {
                            println!("Got error requesting albumart in dir: {}", e);
                        } else {
                            poll_state = PollState::ReadPictureInDir;
                            //println!("polling readpictureindir");
                        }
                    }
                }
            } else {
                println!("INFO: Temporarily failed to acquire lock for writing to tcp stream");
            }
        } else {
            // DEBUG
            //println!("poll_state == {:?}, skipping write...", poll_state);
        }

        if poll_state != PollState::ReadPicture && poll_state != PollState::ReadPictureInDir {
            thread::sleep(Duration::from_millis(50));
        }
    }
    Ok(())
}

fn get_info_from_shared(shared: Arc<Mutex<Shared>>) -> Result<InfoFromShared, ()> {
    let mut filename: String = String::new();
    let mut title: String = String::new();
    let mut artist: String = String::new();
    let mut length: f64 = 0.0;
    let mut pos: f64 = 0.0;
    let mut instant_rec: Instant = Instant::now();
    let mut error_text = String::new();
    if let Ok(lock) = shared.lock() {
        filename = lock.current_song_filename.clone();
        title = lock.current_song_title.clone();
        artist = lock.current_song_artist.clone();
        length = lock.current_song_length;
        pos = lock.current_song_position;
        instant_rec = lock.current_song_pos_rec;
        //println!("Current song: {}", lock.current_song_filename);
        //println!("Current song length: {}", lock.current_song_length);
        //println!("Current song position: {}", lock.current_song_position);
        if !lock.can_authenticate {
            error_text = String::from("Failed to authenticate to mpd");
        } else if !lock.can_get_status {
            error_text = String::from("Failed to get status from mpd");
        } else if !lock.can_get_album_art && !lock.can_get_album_art_in_dir {
            error_text = String::from("Failed to get albumart from mpd");
        }
    }

    Ok(InfoFromShared {
        filename,
        title,
        artist,
        length,
        pos,
        instant_rec,
        error_text,
    })
}

fn window_conf() -> Conf {
    Conf {
        window_title: String::from("mpd info screen"),
        fullscreen: false,
        window_width: 800,
        window_height: 800,
        ..Default::default()
    }
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

#[macroquad::main(window_conf)]
async fn main() -> Result<(), String> {
    let opt = Opt::from_args();
    println!("Got host addr == {}, port == {}", opt.host, opt.port);

    let mut password: Option<String> = opt.password;

    if opt.enable_prompt_password {
        let mut input: String = String::new();
        let mut asterisks: String = String::new();
        let mut dirty: bool = true;
        'prompt_loop: loop {
            draw_text(
                "Input password:",
                TEXT_X_OFFSET,
                TEXT_Y_OFFSET + PROMPT_Y_OFFSET,
                PROMPT_Y_OFFSET,
                WHITE,
            );
            if let Some(k) = get_last_key_pressed() {
                if k == KeyCode::Backspace {
                    input.pop();
                    dirty = true;
                } else if k == KeyCode::Enter {
                    password = Some(input);
                    break 'prompt_loop;
                }
            }
            if let Some(c) = get_char_pressed() {
                input.push(c);
                dirty = true;
            }
            if dirty {
                let input_count = input.chars().count();
                if asterisks.len() < input_count {
                    for _ in 0..(input_count - asterisks.len()) {
                        asterisks.push('*');
                    }
                } else {
                    asterisks.truncate(input_count);
                }
                dirty = false;
            }
            draw_text(
                &asterisks,
                TEXT_X_OFFSET,
                TEXT_Y_OFFSET + PROMPT_Y_OFFSET * 2.0,
                PROMPT_Y_OFFSET,
                WHITE,
            );

            next_frame().await
        }
    }

    let connection = get_connection(opt.host, opt.port)?;
    connection
        .set_read_timeout(Some(Duration::from_millis(50)))
        .expect("Should be able to set timeout for TcpStream reads");
    connection
        .set_write_timeout(Some(Duration::from_secs(1)))
        .expect("Should be able to set timeout for TcpStream writes");

    let shared_data = Arc::new(Mutex::new(Shared::new(connection)));
    if let Some(p) = password {
        shared_data
            .lock()
            .expect("Should be able to get mutex lock")
            .password = p;
    }
    let atomic_dirty_flag = Arc::new(AtomicBool::new(true));
    let thread_shared_data = shared_data.clone();
    let thread_dirty_flag = atomic_dirty_flag.clone();

    let child = thread::spawn(move || {
        info_loop(thread_shared_data, thread_dirty_flag).expect("Failure during info_loop");
    });

    let mut timer: f64 = 0.0;
    let mut track_timer: f64 = 0.0;
    let mut filename: String = String::new();
    let mut title: String = String::new();
    let mut artist: String = String::new();
    let mut art_texture: Option<Texture2D> = None;
    let mut art_draw_params: Option<DrawTextureParams> = None;
    let mut art_draw_width: f32 = 32.0;
    let mut art_draw_height: f32 = 32.0;
    let mut filename_font_size: Option<u16> = None;
    let mut text_dim: TextDimensions = measure_text("undefined", None, 24, 1.0);
    let mut prev_width = screen_width();
    let mut prev_height = screen_height();
    let mut error_text = String::new();
    let mut title_dim_opt: Option<TextDimensions> = None;
    let mut title_font_size: u16 = INITIAL_FONT_SIZE;
    let mut artist_dim_opt: Option<TextDimensions> = None;
    let mut artist_font_size: u16 = ARTIST_INITIAL_FONT_SIZE;
    let mut temp_offset_y: f32;
    let mut check_due_to_track_timer_count: u64 = 0;

    'macroquad_main: loop {
        let dt: f64 = get_frame_time() as f64;
        clear_background(BLACK);

        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
            break 'macroquad_main;
        } else if (prev_width - screen_width()).abs() > SCREEN_DIFF_MARGIN
            || (prev_height - screen_height()).abs() > SCREEN_DIFF_MARGIN
        {
            prev_width = screen_width();
            prev_height = screen_height();
            filename_font_size = None;
            title_dim_opt = None;
            artist_dim_opt = None;
            art_draw_params = None;
        }

        timer -= dt;
        track_timer -= dt;
        if timer < 0.0
            || (track_timer < 0.0 && check_due_to_track_timer_count < CHECK_TRACK_TIMER_MAX_COUNT)
        {
            if track_timer < 0.0 {
                check_due_to_track_timer_count += 1;
                //println!("check_due_to_track_timer_count incremented"); // DEBUG
            }
            timer = CHECK_SHARED_WAIT_TIME;
            let dirty_flag = atomic_dirty_flag.load(Ordering::Relaxed);
            if dirty_flag
                || (track_timer < 0.0
                    && check_due_to_track_timer_count < CHECK_TRACK_TIMER_MAX_COUNT)
            {
                if dirty_flag {
                    atomic_dirty_flag.store(false, Ordering::Relaxed);
                }
                let info_result = get_info_from_shared(shared_data.clone());
                if let Ok(info) = info_result {
                    if info.filename != filename {
                        filename = info.filename;
                        art_texture = None;
                        filename_font_size = None;
                        title.clear();
                        title_dim_opt = None;
                        artist.clear();
                        artist_dim_opt = None;
                        art_draw_params = None;
                        check_due_to_track_timer_count = 0;
                    }
                    let duration_since = info.instant_rec.elapsed();
                    let recorded_time = info.length - info.pos - duration_since.as_secs_f64();
                    if (recorded_time - track_timer).abs() > TIME_MAX_DIFF {
                        track_timer = info.length - info.pos - duration_since.as_secs_f64();
                    }
                    if !info.error_text.is_empty() {
                        error_text = info.error_text;
                    }
                    if !info.title.is_empty() {
                        title = info.title;
                    }
                    if !info.artist.is_empty() {
                        artist = info.artist;
                    }
                }
            }

            if art_texture.is_none() {
                let mut image_result: Option<ImageResult<DynamicImage>> = None;
                {
                    let lock_result = shared_data.lock();
                    if let Ok(l) = lock_result {
                        image_result = Some(image::load_from_memory(&l.art_data));
                    }
                }
                if let Some(Ok(dynimg)) = image_result {
                    let img_buf = dynimg.to_rgba8();
                    art_texture = Some(Texture2D::from_rgba8(
                        img_buf
                            .width()
                            .try_into()
                            .expect("width of image should fit in u16"),
                        img_buf
                            .height()
                            .try_into()
                            .expect("height of image should fit into u16"),
                        &img_buf.to_vec(),
                    ));
                }
            }
        }

        if let Some(texture) = art_texture {
            if texture.width() > prev_width || texture.height() > prev_height {
                if art_draw_params.is_none() {
                    let ratio: f32 = texture.width() / texture.height();
                    // try filling to height
                    art_draw_height = prev_height;
                    art_draw_width = prev_height * ratio;
                    if art_draw_width > prev_width {
                        // try filling to width instead
                        art_draw_width = prev_width;
                        art_draw_height = prev_width / ratio;
                    }

                    art_draw_params = Some(DrawTextureParams {
                        dest_size: Some(Vec2::new(art_draw_width, art_draw_height)),
                        ..Default::default()
                    });
                }
                draw_texture_ex(
                    texture,
                    (prev_width - art_draw_width) / 2.0f32,
                    (prev_height - art_draw_height) / 2.0f32,
                    WHITE,
                    art_draw_params.as_ref().unwrap().clone(),
                );
            } else {
                draw_texture(
                    texture,
                    (prev_width - texture.width()) / 2.0f32,
                    (prev_height - texture.height()) / 2.0f32,
                    WHITE,
                );
            }
        }

        if !is_key_down(KeyCode::H) {
            temp_offset_y = 0.0;
            if !filename.is_empty() && !opt.disable_show_filename {
                if filename_font_size.is_none() {
                    filename_font_size = Some(INITIAL_FONT_SIZE);
                    loop {
                        text_dim = measure_text(
                            &filename,
                            None,
                            *filename_font_size.as_ref().unwrap(),
                            1.0f32,
                        );
                        if text_dim.width + TEXT_X_OFFSET > prev_width {
                            filename_font_size = filename_font_size.map(|s| s - 4);
                        } else {
                            break;
                        }

                        if *filename_font_size.as_ref().unwrap() <= 4 {
                            filename_font_size = Some(4);
                            text_dim = measure_text(
                                &filename,
                                None,
                                *filename_font_size.as_ref().unwrap(),
                                1.0f32,
                            );
                            break;
                        }
                    }
                }
                draw_rectangle(
                    TEXT_X_OFFSET,
                    prev_height - TEXT_Y_OFFSET - text_dim.height,
                    text_dim.width,
                    text_dim.height,
                    Color::new(0.0, 0.0, 0.0, 0.4),
                );
                draw_text(
                    &filename,
                    TEXT_X_OFFSET,
                    prev_height - TEXT_Y_OFFSET,
                    *filename_font_size.as_ref().unwrap() as f32,
                    WHITE,
                );

                temp_offset_y += TEXT_Y_OFFSET + text_dim.height;
            }

            // Get title dimensions early so that artist size is at most title size
            if !title.is_empty() && !opt.disable_show_title && title_dim_opt.is_none() {
                let mut length: u16 = title.chars().count().try_into().unwrap_or(u16::MAX);
                length /= 10;
                if length <= 2 {
                    length = 3;
                }
                let screen_factor = 4 * screen_width() as u16 / TITLE_SCREEN_FACTOR;
                title_font_size = TITLE_INITIAL_FONT_SIZE / length as u16 + screen_factor;
                if title_font_size < TITLE_INITIAL_MIN_FONT_SIZE {
                    title_font_size = TITLE_INITIAL_MIN_FONT_SIZE;
                }
                loop {
                    title_dim_opt = Some(measure_text(&title, None, title_font_size, 1.0f32));
                    if title_dim_opt.as_ref().unwrap().width + TEXT_X_OFFSET > prev_width {
                        title_font_size -= 4;
                        if title_font_size < 4 {
                            title_font_size = 4;
                            title_dim_opt =
                                Some(measure_text(&title, None, title_font_size, 1.0f32));
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }

            if !artist.is_empty() && !opt.disable_show_artist {
                if artist_dim_opt.is_none() {
                    if !title.is_empty() && !opt.disable_show_title {
                        artist_font_size = title_font_size;
                    } else {
                        artist_font_size = ARTIST_INITIAL_FONT_SIZE;
                    }
                    loop {
                        artist_dim_opt =
                            Some(measure_text(&artist, None, artist_font_size, 1.0f32));
                        if artist_dim_opt.as_ref().unwrap().width + TEXT_X_OFFSET > prev_width {
                            artist_font_size -= 4;
                            if artist_font_size < 4 {
                                artist_font_size = 4;
                                artist_dim_opt =
                                    Some(measure_text(&artist, None, artist_font_size, 1.0f32));
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }

                let temp_dim_opt = artist_dim_opt.as_ref().unwrap();
                draw_rectangle(
                    TEXT_X_OFFSET,
                    prev_height - temp_offset_y - TEXT_Y_OFFSET - temp_dim_opt.height,
                    temp_dim_opt.width,
                    temp_dim_opt.height,
                    Color::new(0.0, 0.0, 0.0, 0.4),
                );
                draw_text(
                    &artist,
                    TEXT_X_OFFSET,
                    prev_height - temp_offset_y - TEXT_Y_OFFSET,
                    artist_font_size.into(),
                    WHITE,
                );

                temp_offset_y += TEXT_Y_OFFSET + temp_dim_opt.height;
            }

            if !title.is_empty() && !opt.disable_show_title {
                let temp_dim_opt = title_dim_opt.as_ref().unwrap();
                draw_rectangle(
                    TEXT_X_OFFSET,
                    prev_height - temp_offset_y - TEXT_Y_OFFSET - temp_dim_opt.height,
                    temp_dim_opt.width,
                    temp_dim_opt.height,
                    Color::new(0.0, 0.0, 0.0, 0.4),
                );
                draw_text(
                    &title,
                    TEXT_X_OFFSET,
                    prev_height - temp_offset_y - TEXT_Y_OFFSET,
                    title_font_size.into(),
                    WHITE,
                );

                temp_offset_y += TEXT_Y_OFFSET + temp_dim_opt.height;
            }

            let timer_string = seconds_to_time(track_timer);
            let timer_dim = measure_text(&timer_string, None, TIMER_FONT_SIZE, 1.0f32);
            draw_rectangle(
                TEXT_X_OFFSET,
                prev_height - temp_offset_y - TEXT_Y_OFFSET - timer_dim.height,
                timer_dim.width,
                timer_dim.height,
                Color::new(0.0, 0.0, 0.0, 0.4),
            );
            draw_text(
                &timer_string,
                TEXT_X_OFFSET,
                prev_height - temp_offset_y - TEXT_Y_OFFSET,
                TIMER_FONT_SIZE.into(),
                WHITE,
            );

            if !error_text.is_empty() {
                draw_text(&error_text, 0.0, 32.0f32, 32.0f32, WHITE);
            }
        }

        next_frame().await
    }

    println!("Stopping thread...");
    shared_data
        .lock()
        .map_err(|_| String::from("Failed to get shared_data.thread_running in main"))?
        .thread_running = false;

    //println!("Waiting on thread...");
    thread::sleep(Duration::from_millis(200));

    println!("Joining on thread...");
    child.join().expect("Should be able to join on thread");

    //get_info_from_shared(shared_data.clone(), true)
    //    .expect("Should be able to get info from shared");

    Ok(())
}
