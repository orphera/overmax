//! Foreign-toplevel (wlr) state parsing and unique game target selection.

use super::*;

#[derive(Clone, Default)]
pub(super) struct ForeignToplevelSnapshot {
    pub(super) identity: u64,
    pub(super) title: Option<String>,
    pub(super) activated: bool,
    pub(super) fullscreen: Option<bool>,
    pub(super) outputs: Vec<wl_output::WlOutput>,
}

#[derive(Default)]
pub(super) struct ForeignToplevelState {
    pub(super) pending: ForeignToplevelSnapshot,
    pub(super) committed: Option<ForeignToplevelSnapshot>,
}

pub(super) struct ForeignToplevel {
    pub(super) handle: ZwlrForeignToplevelHandleV1,
    pub(super) state: ForeignToplevelState,
}

#[derive(Default)]
pub(super) struct PresentationTarget {
    pub(super) identity: Option<u64>,
    pub(super) epoch: u64,
}

impl PresentationTarget {
    pub(super) fn observe(&mut self, identity: Option<u64>) -> u64 {
        if self.identity != identity {
            self.identity = identity;
            self.epoch += 1;
        }
        self.epoch
    }
}

pub(super) fn parse_foreign_toplevel_states(
    raw: &[u8],
    supports_fullscreen: bool,
) -> (bool, Option<bool>) {
    let mut activated = false;
    let mut fullscreen = false;
    for bytes in raw.chunks_exact(4) {
        match u32::from_ne_bytes(bytes.try_into().expect("four-byte chunk")) {
            2 => activated = true,
            3 if supports_fullscreen => fullscreen = true,
            _ => {}
        }
    }
    (activated, supports_fullscreen.then_some(fullscreen))
}

pub(super) fn unique_matching_toplevel<'a>(
    title: &str,
    toplevels: impl Iterator<Item = &'a ForeignToplevelState>,
) -> Option<&'a ForeignToplevelSnapshot> {
    let mut matches = toplevels.filter_map(|state| {
        state
            .committed
            .as_ref()
            .filter(|snapshot| snapshot.title.as_deref() == Some(title))
    });
    let target = matches.next()?;
    matches.next().is_none().then_some(target)
}
