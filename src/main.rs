use image::{DynamicImage, ImageResult};
use macroquad::prelude::*;
use std::convert::TryInto;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use structopt::StructOpt;

const POLL_DURATION: Duration = Duration::from_secs(2);
const TITLE_X_OFFSET: f32 = 20.0f32;
const TITLE_Y_OFFSET: f32 = 20.0f32;
const TIMER_X_OFFSET: f32 = 20.0f32;
const TIMER_Y_OFFSET: f32 = 20.0f32;
const TIME_MAX_DIFF: f64 = 2.0f64;

#[derive(StructOpt, Debug)]
#[structopt(name = "mpd_info_screen")]
struct Opt {
    host: Ipv4Addr,
    #[structopt(default_value = "6600")]
    port: u16,
    #[structopt(short = "p")]
    password: Option<String>,
}

struct Shared {
    art_data: Vec<u8>,
    art_data_size: usize,
    current_song: String,
    current_song_length: f64,
    current_song_position: f64,
    current_song_pos_rec: Instant,
    thread_running: bool,
    stream: TcpStream,
    password: String,
    dirty: bool,
}

impl Shared {
    fn new(stream: TcpStream) -> Self {
        Self {
            art_data: Vec::new(),
            art_data_size: 0,
            current_song: String::new(),
            current_song_length: 0.0,
            current_song_position: 0.0,
            current_song_pos_rec: Instant::now(),
            thread_running: true,
            stream,
            password: String::new(),
            dirty: true,
        }
    }
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
        Ok((
            char::from_u32(buf[idx] as u32)
                .ok_or_else(|| (String::from("Not one-byte UTF-8"), 0u8))?,
            1u8,
        ))
    } else if buf[idx] & 0b11100000 == 0b11000000 {
        if idx + 1 >= buf.len() {
            saved.push(buf[idx]);
            return Err((
                String::from("Is two byte UTF-8, but not enough bytes provided"),
                1u8,
            ));
        }
        Ok((
            char::from_u32((buf[idx] as u32) | ((buf[idx + 1] as u32) << 8))
                .ok_or_else(|| (String::from("Not two-byte UTF-8"), 0u8))?,
            2u8,
        ))
    } else if buf[idx] & 0b11110000 == 0b11100000 {
        if idx + 2 >= buf.len() {
            for c in buf.iter().skip(idx) {
                saved.push(*c);
            }
            return Err((
                String::from("Is three byte UTF-8, but not enough bytes provided"),
                (idx + 3 - buf.len()) as u8,
            ));
        }
        Ok((
            char::from_u32(
                (buf[idx] as u32) | ((buf[idx + 1] as u32) << 8) | ((buf[idx + 2] as u32) << 16),
            )
            .ok_or_else(|| (String::from("Not three-byte UTF-8"), 0u8))?,
            3u8,
        ))
    } else if buf[idx] & 0b11111000 == 0b11110000 {
        if idx + 2 >= buf.len() {
            for c in buf.iter().skip(idx) {
                saved.push(*c);
            }
            return Err((
                String::from("Is four byte UTF-8, but not enough bytes provided"),
                (idx + 4 - buf.len()) as u8,
            ));
        }
        Ok((
            char::from_u32(
                (buf[idx] as u32)
                    | ((buf[idx + 1] as u32) << 8)
                    | ((buf[idx + 2] as u32) << 16)
                    | ((buf[idx + 3] as u32) << 24),
            )
            .ok_or_else(|| (String::from("Not four-byte UTF-8"), 0u8))?,
            4u8,
        ))
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

fn debug_write_albumart_to_file(data: &[u8]) -> Result<(), String> {
    let mut f = File::create("albumartOut.jpg")
        .map_err(|_| String::from("Failed to open file for writing albumart"))?;
    f.write_all(data)
        .map_err(|_| String::from("Failed to write to albumartOut.jpg"))?;

    Ok(())
}

fn info_loop(shared_data: Arc<Mutex<Shared>>) -> Result<(), String> {
    let mut buf: [u8; 4192] = [0; 4192];
    let mut init: bool = true;
    let mut saved: Vec<u8> = Vec::new();
    let mut saved_str: String = String::new();
    let mut authenticated: bool = false;
    let mut song_title_get_time: Instant = Instant::now() - Duration::from_secs(10);
    let mut song_pos_get_time: Instant = Instant::now() - Duration::from_secs(10);
    let mut song_length_get_time: Instant = Instant::now() - Duration::from_secs(10);
    let mut current_binary_size: usize = 0;
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
                    let mut read_vec: Vec<u8> = Vec::from(buf);
                    read_vec.resize(count, 0);
                    loop {
                        let mut count = read_vec.len();
                        if current_binary_size > 0 {
                            if current_binary_size <= count {
                                lock.art_data
                                    .extend_from_slice(&read_vec[0..current_binary_size]);
                                read_vec = read_vec.split_off(current_binary_size + 1);
                                count = read_vec.len();
                                current_binary_size = 0;
                                //println!(
                                //    "art_data len is {} after fully reading",
                                //    lock.art_data.len()
                                //);
                                // TODO Debug
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
                                    break;
                                } else {
                                    return Err(String::from(
                                        "Did not get expected init message from MPD",
                                    ));
                                }
                            } else {
                                //println!("Got response: {}", line);
                                if line.starts_with("OK") {
                                    break;
                                } else if line.starts_with("file: ") {
                                    let song_file = line.split_off(6);
                                    if song_file != lock.current_song {
                                        lock.current_song = song_file;
                                        lock.art_data.clear();
                                        lock.art_data_size = 0;
                                        //println!("Got different song file, clearing art_data...");
                                    }
                                    lock.dirty = true;
                                    song_title_get_time = Instant::now();
                                } else if line.starts_with("elapsed: ") {
                                    let parse_pos_result = f64::from_str(&line.split_off(9));
                                    if let Ok(value) = parse_pos_result {
                                        lock.current_song_position = value;
                                        lock.dirty = true;
                                        song_pos_get_time = Instant::now();
                                        lock.current_song_pos_rec = Instant::now();
                                    } else {
                                        println!("Got error trying to get current_song_position");
                                    }
                                } else if line.starts_with("duration: ") {
                                    let parse_pos_result = f64::from_str(&line.split_off(10));
                                    if let Ok(value) = parse_pos_result {
                                        lock.current_song_length = value;
                                        lock.dirty = true;
                                        song_length_get_time = Instant::now();
                                    }
                                } else if line.starts_with("size: ") {
                                    let parse_artsize_result = usize::from_str(&line.split_off(6));
                                    if let Ok(value) = parse_artsize_result {
                                        lock.art_data_size = value;
                                        lock.dirty = true;
                                    }
                                } else if line.starts_with("binary: ") {
                                    let parse_artbinarysize_result =
                                        usize::from_str(&line.split_off(8));
                                    if let Ok(value) = parse_artbinarysize_result {
                                        current_binary_size = value;
                                    }
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
                println!("Failed to acquire lock for reading to stream");
            }
        }

        // write block
        {
            let lock_result = shared_data.try_lock();
            if let Ok(mut lock) = lock_result {
                if !authenticated && !lock.password.is_empty() {
                    let p = lock.password.clone();
                    let write_result = lock.stream.write(format!("password {}\n", p).as_bytes());
                    if write_result.is_ok() {
                        authenticated = true;
                    } else if let Err(e) = write_result {
                        println!("Got error requesting authentication: {}", e);
                    }
                } else if song_title_get_time.elapsed() > POLL_DURATION {
                    let write_result = lock.stream.write(b"currentsong\n");
                    if let Err(e) = write_result {
                        println!("Got error requesting currentsong info: {}", e);
                    }
                } else if song_length_get_time.elapsed() > POLL_DURATION
                    || song_pos_get_time.elapsed() > POLL_DURATION
                {
                    let write_result = lock.stream.write(b"status\n");
                    if let Err(e) = write_result {
                        println!("Got error requesting status: {}", e);
                    }
                } else if (lock.art_data.is_empty() || lock.art_data.len() != lock.art_data_size)
                    && !lock.current_song.is_empty()
                {
                    let title = lock.current_song.clone();
                    let art_data_length = lock.art_data.len();
                    let write_result = lock.stream.write(
                        format!("readpicture \"{}\" {}\n", title, art_data_length).as_bytes(),
                    );
                    if let Err(e) = write_result {
                        println!("Got error requesting albumart: {}", e);
                    }
                }
            } else {
                println!("Failed to acquire lock for writing to stream");
            }
        }

        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn get_info_from_shared(
    shared: Arc<Mutex<Shared>>,
    force_check: bool,
) -> Result<(String, f64, f64, Instant), String> {
    let mut title: String = String::new();
    let mut length: f64 = 0.0;
    let mut pos: f64 = 0.0;
    let mut instant_rec: Instant = Instant::now();
    if let Ok(mut lock) = shared.lock() {
        if lock.dirty || force_check {
            title = lock.current_song.clone();
            length = lock.current_song_length;
            pos = lock.current_song_position;
            instant_rec = lock.current_song_pos_rec;
            //println!("Current song: {}", lock.current_song);
            //println!("Current song length: {}", lock.current_song_length);
            //println!("Current song position: {}", lock.current_song_position);
            lock.dirty = false;
        }
    }

    Ok((title, length, pos, instant_rec))
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

    let connection = get_connection(opt.host, opt.port)?;
    connection
        .set_read_timeout(Some(Duration::from_millis(50)))
        .expect("Should be able to set timeout for TcpStream reads");
    connection
        .set_write_timeout(Some(Duration::from_secs(1)))
        .expect("Should be able to set timeout for TcpStream writes");

    let shared_data = Arc::new(Mutex::new(Shared::new(connection)));
    if let Some(p) = opt.password {
        shared_data
            .lock()
            .expect("Should be able to get mutex lock")
            .password = p;
    }
    let thread_shared_data = shared_data.clone();

    let child = thread::spawn(move || {
        info_loop(thread_shared_data).expect("Failure during info_loop");
    });

    let wait_time: f64 = 0.75;
    let mut timer: f64 = 0.0;
    let mut track_timer: f64 = 0.0;
    let mut title: String = String::new();
    let mut art_texture: Option<Texture2D> = None;

    'macroquad_main: loop {
        let dt: f64 = get_frame_time() as f64;
        clear_background(BLACK);

        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
            break 'macroquad_main;
        }

        timer -= dt;
        track_timer -= dt;
        if timer < 0.0 || track_timer < 0.0 {
            timer = wait_time;
            let info_result = get_info_from_shared(shared_data.clone(), true);
            if let Ok((track_title, duration, pos, instant_rec)) = info_result {
                if track_title != title {
                    title = track_title;
                    art_texture = None;
                }
                let duration_since = instant_rec.elapsed();
                let recorded_time = duration - pos - duration_since.as_secs_f64();
                if (recorded_time - track_timer).abs() > TIME_MAX_DIFF {
                    track_timer = duration - pos;
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
            if texture.width() > screen_width() || texture.height() > screen_height() {
                let ratio: f32 = texture.width() / texture.height();
                // try filling to height
                let mut height = screen_height();
                let mut width = screen_height() * ratio;
                if width > screen_width() {
                    // try filling to width instead
                    width = screen_width();
                    height = screen_width() / ratio;
                }

                let draw_params: DrawTextureParams = DrawTextureParams {
                    dest_size: Some(Vec2::new(width, height)),
                    ..Default::default()
                };
                draw_texture_ex(
                    texture,
                    (screen_width() - width) / 2.0f32,
                    (screen_height() - height) / 2.0f32,
                    WHITE,
                    draw_params,
                );
            } else {
                draw_texture(
                    texture,
                    (screen_width() - texture.width()) / 2.0f32,
                    (screen_height() - texture.height()) / 2.0f32,
                    WHITE,
                );
            }
        }

        if !title.is_empty() {
            let mut text_size = 64;
            let mut text_dim: TextDimensions;
            loop {
                text_dim = measure_text(&title, None, text_size, 1.0f32);
                if text_dim.width + TITLE_X_OFFSET > screen_width() {
                    text_size -= 4;
                } else {
                    break;
                }

                if text_size <= 4 {
                    text_size = 4;
                    break;
                }
            }
            draw_rectangle(
                TITLE_X_OFFSET,
                screen_height() - TITLE_Y_OFFSET - text_dim.height * 2.0,
                text_dim.width,
                text_dim.height,
                Color::new(0.0, 0.0, 0.0, 0.4),
            );
            draw_text(
                &title,
                TITLE_X_OFFSET,
                screen_height() - TITLE_Y_OFFSET - text_dim.height,
                text_size as f32,
                WHITE,
            );

            let timer_string = seconds_to_time(track_timer);
            let timer_dim = measure_text(&timer_string, None, 64, 1.0f32);
            draw_rectangle(
                TIMER_X_OFFSET,
                screen_height()
                    - TITLE_Y_OFFSET
                    - text_dim.height
                    - TIMER_Y_OFFSET
                    - timer_dim.height * 2.0,
                timer_dim.width,
                timer_dim.height,
                Color::new(0.0, 0.0, 0.0, 0.4),
            );
            draw_text(
                &timer_string,
                TIMER_X_OFFSET,
                screen_height()
                    - TITLE_Y_OFFSET
                    - text_dim.height
                    - TIMER_Y_OFFSET
                    - timer_dim.height,
                64.0f32,
                WHITE,
            );
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

    get_info_from_shared(shared_data.clone(), true)
        .expect("Should be able to get info from shared");

    Ok(())
}
