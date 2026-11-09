#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiCommand {
    OpenSettings,
    OpenDebug,
    OpenSync,
    OpenUploadHistory,
    Exit,
    UploadCurrentPattern,
    SetOverlayPosition { x: i32, y: i32 },
    SetOverlayPositionAndManualSnap { x: i32, y: i32 },
}
