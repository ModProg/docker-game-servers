#[macro_export]
macro_rules! exit {
    () => {
        std::process::exit(0);
    };
    ($ec:expr, $($message:expr), +) => {
        {eprintln!($($message), +);
        std::process::exit($ec as i32);}
    };
}

#[macro_export]
macro_rules! warning {
    ($($message:expr), +) => {
        eprintln!("[WARNING] {}", format!($($message), +));
    };
}
