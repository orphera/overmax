#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct CapturedFrame {
    pub width: i32,
    pub height: i32,
    pub bgra: Vec<u8>,
}
