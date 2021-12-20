use std::fmt::Display;
use structopt::clap::arg_enum;

arg_enum! {
    #[derive(Copy, Clone, Debug, PartialEq)]
    pub enum LogState {
        ERROR,
        WARNING,
        DEBUG,
        VERBOSE,
    }
}

pub fn log<T>(msg: T, level: LogState, state: LogState)
where
    T: Display,
{
    if level == LogState::ERROR {
        log_error(msg);
    } else if level == LogState::WARNING {
        if state != LogState::ERROR {
            log_warning(msg);
        }
    } else if level == LogState::DEBUG {
        if state == LogState::DEBUG || state == LogState::VERBOSE {
            log_debug(msg);
        }
    } else if level == LogState::VERBOSE {
        if state == LogState::VERBOSE {
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
