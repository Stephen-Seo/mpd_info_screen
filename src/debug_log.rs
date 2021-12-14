use std::fmt::Display;

#[cfg(debug_assertions)]
pub fn log<T>(msg: T) -> ()
where
    T: Display,
{
    println!("{}", msg);
}

#[cfg(not(debug_assertions))]
pub fn log<T>(msg: T) -> ()
where
    T: Display,
{
    // intentionally left blank, no logging in debug mode
}
