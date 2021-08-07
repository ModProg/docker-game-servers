#[macro_export]
macro_rules! warning {
    ($($message:expr), +) => {
        eprintln!("[WARNING] {}", format!($($message), +));
    };
}
