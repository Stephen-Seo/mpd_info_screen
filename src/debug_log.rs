use std::fmt::Display;
use structopt::clap::arg_enum;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogState {
    Error,
    Warning,
    Debug,
    Verbose,
}

arg_enum! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub enum LogLevel {
        Error,
        Warning,
        Debug,
        Verbose,
    }
}

pub fn log<T>(msg: T, state: LogState, level: LogLevel)
where
    T: Display,
{
    if state == LogState::Error {
        log_error(msg);
    } else if state == LogState::Warning {
        if level != LogLevel::Error {
            log_warning(msg);
        }
    } else if state == LogState::Debug {
        if level == LogLevel::Debug || level == LogLevel::Verbose {
            log_debug(msg);
        }
    } else if state == LogState::Verbose {
        if level == LogLevel::Verbose {
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
    println!("Error: {}", msg);
}

pub fn log_warning<T>(msg: T)
where
    T: Display,
{
    println!("Warning: {}", msg);
}

pub fn log_debug<T>(msg: T)
where
    T: Display,
{
    println!("Debug: {}", msg);
}

pub fn log_verbose<T>(msg: T)
where
    T: Display,
{
    println!("Verbose: {}", msg);
}
