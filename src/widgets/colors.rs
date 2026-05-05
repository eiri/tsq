use vizia::vg;

/// Convert ARGB components into a [`vg::Color`].
#[inline]
pub fn argb(a: u8, r: u8, g: u8, b: u8) -> vg::Color {
    vg::Color::from_argb(a, r, g, b)
}

/// Convenience: unpack a colour tuple and call `argb`.
#[inline]
pub fn c(col: (u8, u8, u8, u8)) -> vg::Color {
    argb(col.0, col.1, col.2, col.3)
}

pub const PIP_OFF_0: (u8, u8, u8, u8) = (255, 80, 78, 55);
pub const PIP_OFF_1: (u8, u8, u8, u8) = (255, 45, 44, 30);
pub const PIP_OFF_2: (u8, u8, u8, u8) = (255, 25, 24, 15);

pub const PIP_ON_R: u8 = 255;
pub const PIP_ON_G: u8 = 5;
pub const PIP_ON_B: u8 = 3;
pub const PIP_ON_G_BRIGHT: u8 = 65; // PIP_ON_G + 60

pub const PIP_RIM_0: (u8, u8, u8, u8) = (0, 255, 160, 0);
pub const PIP_RIM_1: (u8, u8, u8, u8) = (130, 255, 200, 20);

pub const PIP_BORDER_TOP: (u8, u8, u8, u8) = (180, 210, 205, 160);
pub const PIP_BORDER_BOT: (u8, u8, u8, u8) = (120, 30, 28, 18);
pub const PIP_BORDER_SIDE: (u8, u8, u8, u8) = (100, 130, 128, 90);

pub const HEART_OFF_0: (u8, u8, u8, u8) = (255, 72, 70, 48);
pub const HEART_OFF_1: (u8, u8, u8, u8) = (255, 40, 39, 26);
pub const HEART_OFF_2: (u8, u8, u8, u8) = (255, 22, 21, 12);

pub const HEART_ON_R: u8 = 255;
pub const HEART_ON_G: u8 = 5;
pub const HEART_ON_B: u8 = 3;
pub const HEART_ON_G_BRIGHT: u8 = 65;

pub const HEART_RIM_0: (u8, u8, u8, u8) = (0, 255, 160, 0);
pub const HEART_RIM_1: (u8, u8, u8, u8) = (130, 255, 200, 20);

pub const HEART_BORDER_TOP: (u8, u8, u8, u8) = (180, 210, 205, 160);
pub const HEART_BORDER_BOT: (u8, u8, u8, u8) = (120, 30, 28, 18);
pub const HEART_BORDER_SIDE: (u8, u8, u8, u8) = (100, 130, 128, 90);

/// Outer halo - RGB base (alpha is computed from glow_alpha at runtime)
pub const DOT_HALO_R: (u8, u8, u8) = (220, 30, 10);
pub const DOT_HALO_M: (u8, u8, u8) = (180, 20, 5);

/// Bezel rim gradient (top → mid → bottom)
pub const DOT_BEZEL_0: (u8, u8, u8, u8) = (255, 210, 208, 175);
pub const DOT_BEZEL_1: (u8, u8, u8, u8) = (255, 175, 172, 140);
pub const DOT_BEZEL_2: (u8, u8, u8, u8) = (255, 130, 128, 100);

/// Pit interior - outer warm highlight (alpha always 255, rgb computed)
pub const DOT_PIT_EDGE: (u8, u8, u8, u8) = (255, 180, 175, 110);

/// LED hot-spot specular highlight RGB (alpha computed at runtime)
pub const DOT_SPEC: (u8, u8, u8) = (255, 200, 180);

/// LED off - flat dark fill
pub const DOT_LED_OFF: (u8, u8, u8, u8) = (255, 18, 8, 8);

/// Rim crescent highlight
pub const DOT_CRESCENT: (u8, u8, u8, u8) = (60, 200, 200, 200);
