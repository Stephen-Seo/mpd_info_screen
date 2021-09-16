use std::fs::File;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use structopt::StructOpt;

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
            for tidx in idx..buf.len() {
                saved.push(buf[tidx]);
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
            for tidx in idx..buf.len() {
                saved.push(buf[tidx]);
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
        let next_char_result = check_next_chars(&mut buf_to_read, idx, saved);
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

fn debug_write_albumart_to_file(data: &Vec<u8>) -> Result<(), String> {
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
                                println!(
                                    "art_data len is {} after fully reading",
                                    lock.art_data.len()
                                );
                                // TODO Debug
                                //let write_file_result = debug_write_albumart_to_file(&lock.art_data);
                            } else {
                                lock.art_data.extend_from_slice(&read_vec[0..count]);
                                current_binary_size -= count;
                                println!("art_data len is {}", lock.art_data.len());
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
                                println!("Got response: {}", line);
                                if line.starts_with("OK") {
                                    break;
                                } else if line.starts_with("file: ") {
                                    let song_file = line.split_off(6);
                                    if song_file != lock.current_song {
                                        lock.current_song = song_file;
                                        lock.art_data.clear();
                                        lock.art_data_size = 0;
                                        println!("Got different song file, clearing art_data...");
                                    }
                                    lock.dirty = true;
                                    song_title_get_time = Instant::now();
                                } else if line.starts_with("elapsed: ") {
                                    let parse_pos_result = f64::from_str(&line.split_off(9));
                                    if let Ok(value) = parse_pos_result {
                                        lock.current_song_position = value;
                                        lock.dirty = true;
                                        song_pos_get_time = Instant::now();
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
                } else if song_title_get_time.elapsed() > Duration::from_secs(5) {
                    let write_result = lock.stream.write(b"currentsong\n");
                    if let Err(e) = write_result {
                        println!("Got error requesting currentsong info: {}", e);
                    }
                } else if song_length_get_time.elapsed() > Duration::from_secs(5)
                    || song_pos_get_time.elapsed() > Duration::from_secs(5)
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

fn get_info_from_shared(shared: Arc<Mutex<Shared>>, force_check: bool) -> Result<(), String> {
    if let Ok(mut lock) = shared.lock() {
        if lock.dirty || force_check {
            println!("Current song: {}", lock.current_song);
            println!("Current song length: {}", lock.current_song_length);
            println!("Current song position: {}", lock.current_song_position);
            lock.dirty = false;
        }
    }

    Ok(())
}

fn main() -> Result<(), String> {
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

    thread::sleep(Duration::from_secs(2));

    get_info_from_shared(shared_data.clone(), false)
        .expect("Should be able to get info from shared");

    thread::sleep(Duration::from_secs(10));

    println!("Stopping thread...");
    shared_data
        .lock()
        .map_err(|_| String::from("Failed to get shared_data.thread_running in main"))?
        .thread_running = false;

    println!("Waiting on thread...");
    thread::sleep(Duration::from_secs(2));

    println!("Joining on thread...");
    child.join().expect("Should be able to join on thread");

    get_info_from_shared(shared_data.clone(), true)
        .expect("Should be able to get info from shared");

    Ok(())
}
