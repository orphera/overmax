#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        println!($($arg)*);
    };
}

pub mod capture;
pub mod detector;
pub use overmax_frame as frame;
pub mod image_index_jacket;
