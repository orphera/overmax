//! Reference Button Mode Colors for Song Select & Result Recognition.

use overmax_cv::Bgr;

/// Representative Freestyle Button Mode Reference Colors for Color Bar [4B, 5B, 6B, 8B]
pub const FREESTYLE_RESULT_MODE_COLORS: [Bgr; 4] = [
    Bgr::from_rgb_hex(0x34D476), // 4B #34D476
    Bgr::from_rgb_hex(0x48BCE1), // 5B #48BCE1
    Bgr::from_rgb_hex(0xDF923B), // 6B #DF923B
    Bgr::from_rgb_hex(0x8592F4), // 8B #8592F4
];

/// Representative Freestyle Button Mode Reference Colors [4B, 5B, 6B, 8B]
pub const FREESTYLE_MODE_COLORS: [Bgr; 4] = [
    Bgr::from_rgb_hex(0x0E4960), // 4B #0E4960
    Bgr::from_rgb_hex(0x44A9C6), // 5B #44A9C6
    Bgr::from_rgb_hex(0xED9430), // 6B #ED9430
    Bgr::from_rgb_hex(0x1D1431), // 8B #1D1431
];

/// Representative OpenMatch Button Mode Reference Colors [4B, 5B, 6B, 8B]
pub const OPENMATCH_MODE_COLORS: [Bgr; 4] = [
    Bgr::from_rgb_hex(0x2E7666), // 4B #2E7666
    Bgr::from_rgb_hex(0x5F8893), // 5B #5F8893
    Bgr::from_rgb_hex(0xC0893D), // 6B #C0893D
    Bgr::from_rgb_hex(0x585A99), // 8B #585A99
];
