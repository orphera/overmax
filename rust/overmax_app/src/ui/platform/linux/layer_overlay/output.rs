//! Output selection, geometry and render/buffer scale helpers.

use super::*;

pub(super) fn select_committed_output<'a, T: PartialEq>(
    outputs: &'a [T],
    current: Option<&T>,
) -> Option<&'a T> {
    match outputs {
        [output] => Some(output),
        outputs => current.and_then(|current| outputs.iter().find(|output| *output == current)),
    }
}

pub(super) fn output_render_scale(
    current: f64,
    estimated: Option<f64>,
    has_viewporter: bool,
    preferred_received: bool,
) -> f64 {
    if preferred_received {
        current
    } else if has_viewporter {
        estimated.unwrap_or(1.0)
    } else {
        estimated.unwrap_or(1.0).round().max(1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct OutputGeometry {
    pub(super) rect: WindowRect,
    pub(super) scale: f64,
}

pub(super) fn output_geometry(info: &OutputInfo) -> Option<OutputGeometry> {
    let current_mode = info.modes.iter().find(|mode| mode.current);
    let (position, logical_size) = match (info.logical_position, info.logical_size) {
        (Some(position), Some(size)) if size.0 > 0 && size.1 > 0 => (position, size),
        _ => {
            let mode = current_mode?;
            let scale = info.scale_factor.max(1);
            (
                info.location,
                (mode.dimensions.0 / scale, mode.dimensions.1 / scale),
            )
        }
    };
    let scale = current_mode.map_or(info.scale_factor.max(1) as f64, |mode| {
        let physical_area = f64::from(mode.dimensions.0) * f64::from(mode.dimensions.1);
        let logical_area = f64::from(logical_size.0) * f64::from(logical_size.1);
        (physical_area / logical_area).sqrt()
    });
    Some(OutputGeometry {
        rect: WindowRect {
            left: position.0,
            top: position.1,
            width: logical_size.0,
            height: logical_size.1,
        },
        scale: scale.max(1.0),
    })
}

pub(super) fn select_output_by_overlap<T>(
    game_rect: WindowRect,
    outputs: impl Iterator<Item = (T, WindowRect)>,
) -> Option<T> {
    outputs
        .map(|(output, rect)| {
            let width = (game_rect.left + game_rect.width).min(rect.left + rect.width)
                - game_rect.left.max(rect.left);
            let height = (game_rect.top + game_rect.height).min(rect.top + rect.height)
                - game_rect.top.max(rect.top);
            (output, i64::from(width.max(0)) * i64::from(height.max(0)))
        })
        .max_by_key(|(_, overlap)| *overlap)
        .filter(|(_, overlap)| *overlap > 0)
        .map(|(output, _)| output)
}

pub(super) fn physical_size(logical: (u32, u32), scale: f64) -> (u32, u32) {
    (
        (f64::from(logical.0) * scale).ceil().max(1.0) as u32,
        (f64::from(logical.1) * scale).ceil().max(1.0) as u32,
    )
}

pub(super) fn fallback_buffer_scale(render_scale: f64, has_viewporter: bool) -> i32 {
    if has_viewporter {
        1
    } else {
        render_scale.round().max(1.0) as i32
    }
}
