use super::upload_result_message;
use crate::ui::platform::native_options;
use overmax_runtime::varchive_jobs::{UploadCacheError, UploadOutcome};

#[test]
fn upload_outcome_maps_to_channel_message() {
    let uploaded = (95.5, false, Some(90.0), Some(false), None);
    let (status, msg, values) = upload_result_message(
        UploadOutcome::Uploaded {
            updated: true,
            cache: Ok(Some(12)),
        },
        overmax_core::Mode::B4,
        uploaded,
    );
    assert_eq!(status, "success");
    let place = crate::t!(
        "sys-place-achieved",
        mode = overmax_core::Mode::B4,
        rank = 12
    );
    let base = crate::t!("status-updated").to_string();
    assert_eq!(
        msg,
        crate::t!(
            "sys-upload-msg-with-rank",
            message = &base,
            rank_msg = &place
        )
    );
    assert_eq!(values.4, Some(12));

    let (status, msg, values) = upload_result_message(
        UploadOutcome::Uploaded {
            updated: false,
            cache: Err(UploadCacheError::FallbackFailed("busy".into())),
        },
        overmax_core::Mode::B4,
        uploaded,
    );
    assert_eq!(status, "success");
    let inner = crate::t!("sys-fallback-cache-failed", error = "busy");
    assert_eq!(msg, crate::t!("sys-upload-cache-error", error = inner));
    assert_eq!(values, uploaded);

    let (status, msg, _) = upload_result_message(
        UploadOutcome::Rejected("denied".into()),
        overmax_core::Mode::B4,
        uploaded,
    );
    assert_eq!((status, msg.as_str()), ("error", "denied"));
}

#[test]
fn main_overlay_stays_out_of_taskbar() {
    let options = native_options(&overmax_data::Settings::default());

    assert_eq!(options.viewport.taskbar, Some(false));
}
