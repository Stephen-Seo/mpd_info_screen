use std::fmt::Display;
use structopt::clap::arg_enum;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LogState {
    ERROR,
    WARNING,
    DEBUG,
    VERBOSE,
}

arg_enum! {
    #[derive(Copy, Clone, Debug, PartialEq)]
    pub enum LogLevel {
        ERROR,
        WARNING,
        DEBUG,
        VERBOSE,
    }
}

pub fn log<T>(msg: T, state: LogState, level: LogLevel)
where
    T: Display,
{
    if state == LogState::ERROR {
        log_error(msg);
    } else if state == LogState::WARNING {
        if level != LogLevel::ERROR {
            log_warning(msg);
        }
    } else if state == LogState::DEBUG {
        if level == LogLevel::DEBUG || level == LogLevel::VERBOSE {
            log_debug(msg);
        }
    } else if state == LogState::VERBOSE {
        if level == LogLevel::VERBOSE {
            log_verbose(msg);
        }
    } else {
        unreachable!();
    }
}

pub fn log_error<T>(msg: T)
where
    T: Display,
{
    println!("ERROR: {}", msg);
}

pub fn log_warning<T>(msg: T)
where
    T: Display,
{
    println!("WARNING: {}", msg);
}

pub fn log_debug<T>(msg: T)
where
    T: Display,
{
    println!("DEBUG: {}", msg);
}

pub fn log_verbose<T>(msg: T)
where
    T: Display,
{
    println!("VERBOSE: {}", msg);
}
