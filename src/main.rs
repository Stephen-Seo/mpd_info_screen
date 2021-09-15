use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "mpd_info_screen")]
struct Opt {
    host: Ipv4Addr,
    #[structopt(default_value = "6600")]
    port: u16,
}

struct Shared {
    art_data: Vec<u8>,
    current_song: String,
    current_song_length: u64,
    current_song_position: u64,
    thread_running: bool,
    stream: TcpStream,
}

impl Shared {
    fn new(stream: TcpStream) -> Self {
        Self {
            art_data: Vec::new(),
            current_song: String::new(),
            current_song_length: 0,
            current_song_position: 0,
            thread_running: true,
            stream,
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

fn read_line(buf: &[u8], count: usize, saved: &mut Vec<u8>) -> Result<String, (String, String)> {
    let mut result = String::new();

    if !saved.is_empty() {
        // TODO
    }

    saved.clear();

    let mut skip_count = 0;
    for idx in 0..count {
        if skip_count > 0 {
            skip_count -= 1;
            continue;
        }
        let next_char_result = check_next_chars(buf, idx, saved);
        if let Ok((c, s)) = next_char_result {
            if c == '\n' {
                return Ok(result);
            }
            result.push(c);
            skip_count = s - 1;
        } else if let Err((msg, count)) = next_char_result {
            for i in 0..count {
                saved.push(buf[idx + i as usize]);
            }
            return Err((String::from("Not enough bytes"), result));
        } else {
            unreachable!();
        }
    }

    Err((String::from("Newline not reached"), result))
}

fn info_loop(shared_data: Arc<Mutex<Shared>>) -> Result<(), String> {
    let mut buf: [u8; 4192] = [0; 4192];
    let mut init: bool = true;
    let mut saved: Vec<u8> = Vec::new();
    let mut saved_str: String = String::new();
    loop {
        if !shared_data.lock().map_err(|_| String::from("Failed to get shared_data.thread_running"))?.thread_running {
            break;
        }
        {
            let lock_result = shared_data.try_lock();
            if let Ok(mut lock) = lock_result {
                let read_result = lock.stream.read(&mut buf);
                if let Ok(count) = read_result {
                    let read_line_result = read_line(&buf, count, &mut saved);
                    if let Ok(mut line) = read_line_result {
                        line = saved_str + &line;
                        saved_str = String::new();
                        if init {
                            if line.starts_with("OK MPD ") {
                                init = false;
                                println!("Got initial \"OK\" from MPD");
                            } else {
                                return Err(String::from("Did not get expected init message from MPD"));
                            }
                        } else {
                            // TODO handling of other messages
                        }
                    } else if let Err((msg, read_line_in_progress)) = read_line_result {
                        println!("Error during \"read_line\": {}", msg);
                        saved_str = read_line_in_progress;
                    } else {
                        unreachable!();
                    }
                }
            }
        }

        // TODO send messages to get info
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let opt = Opt::from_args();
    println!("Got host addr == {}, port == {}", opt.host, opt.port);

    let connection = get_connection(opt.host, opt.port)?;
    connection.set_read_timeout(Some(Duration::from_millis(100))).expect("Should be able to set timeout for TcpStream reads");

    let shared_data = Arc::new(Mutex::new(Shared::new(connection)));
    let thread_shared_data = shared_data.clone();

    let child = thread::spawn(move || {
        info_loop(thread_shared_data).expect("Failure during info_loop");
    });

    thread::sleep(Duration::from_secs(5));

    println!("Stopping thread...");
    shared_data.lock().map_err(|_| String::from("Failed to get shared_data.thread_running in main"))?.thread_running = false;

    println!("Waiting on thread...");
    thread::sleep(Duration::from_secs(5));

    println!("Joining on thread...");
    child.join().expect("Should be able to join on thread");

    Ok(())
}
