#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiCommand {
    OpenSettings,
    OpenDebug,
    OpenSync,
    Exit,
    UploadCurrentPattern,
    SetOverlayPosition { x: i32, y: i32 },
}
