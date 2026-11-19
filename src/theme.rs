//! Colors shared by the native UI and the HTML export.

pub type Rgb = [u8; 3];

pub const BACKGROUND: Rgb = hex(0x25_25_26);
pub const FOREGROUND: Rgb = hex(0xcc_cc_cc);
pub const WIDGET_BG: Rgb = hex(0x3c_3c_3c);
pub const BUTTON_BG: Rgb = hex(0x3e_3e_42);
pub const HOVER_BG: Rgb = hex(0x50_50_50);
pub const ACCENT: Rgb = hex(0x00_7a_cc);
pub const BORDER: Rgb = hex(0x44_44_44);
pub const DISABLED_FG: Rgb = hex(0x66_66_66);

/// Series colors (matplotlib "tab20" order, as used by the original tool).
pub const PALETTE: [Rgb; 20] = [
    hex(0x1f77b4),
    hex(0xff7f0e),
    hex(0x2ca02c),
    hex(0xd62728),
    hex(0x9467bd),
    hex(0x8c564b),
    hex(0xe377c2),
    hex(0x7f7f7f),
    hex(0xbcbd22),
    hex(0x17becf),
    hex(0xaec7e8),
    hex(0xffbb78),
    hex(0x98df8a),
    hex(0xff9896),
    hex(0xc5b0d5),
    hex(0xc49c94),
    hex(0xf7b6d3),
    hex(0xc7c7c7),
    hex(0xdbdb8d),
    hex(0x9edae5),
];

pub fn series_color(index: usize) -> Rgb {
    PALETTE[index % PALETTE.len()]
}

pub fn css(rgb: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

const fn hex(v: u32) -> Rgb {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}
