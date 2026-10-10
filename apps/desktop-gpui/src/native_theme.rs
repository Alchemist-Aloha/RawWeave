//! The RawWeave design tokens, mirroring `DESIGN.md`.
//!
//! Presentation only: no processing value, node identifier, or persisted state
//! passes through here. Each constant is the DESIGN.md token of the same name,
//! so a colour in this shell can be traced to the committed world instead of
//! being invented at the call site.

// The token table is complete on purpose: it is the design system's vocabulary,
// and a surface spends only the part it needs.
#![allow(dead_code)]

use gpui::{App, FontWeight, Pixels, SharedString, TextSystem, px};
use rawweave_gpui::batchqueue::Tone;

// --- room: warm near-black handling chrome ---------------------------------
pub const ROOM_GROUND: u32 = 0x14110d;
pub const ROOM_SUNK: u32 = 0x100e0a;
pub const ROOM_STRIP: u32 = 0x171310;
pub const ROOM_PANEL: u32 = 0x1a1611;
pub const ROOM_RAISE: u32 = 0x241e16;
pub const ROOM_HOVER: u32 = 0x302820;
pub const ROOM_SELECT: u32 = 0x2f2a20;
pub const ROOM_LINE: u32 = 0x332a21;
pub const ROOM_LINE_STRONG: u32 = 0x443a2d;
pub const ROOM_LINE_FIELD: u32 = 0x3a3126;
pub const ROOM_INK: u32 = 0xf5f0e4;
pub const ROOM_INK_BODY: u32 = 0xded6c4;
pub const ROOM_INK_DIM: u32 = 0xa89d88;
pub const ROOM_INK_FAINT: u32 = 0x847a66;

// --- bench: the one plane the frames are laid out on (dark by default) -----
pub const BENCH_GROUND: u32 = 0x1b1c1d;
pub const BENCH_RAISE: u32 = 0x24262a;
pub const BENCH_HOVER: u32 = 0x2c2f33;
pub const BENCH_SUNK: u32 = 0x101112;
pub const BENCH_GRID: u32 = 0x333538;
pub const BENCH_LINE: u32 = 0x3b3d40;
pub const BENCH_LINE_STRONG: u32 = 0x55585c;
pub const BENCH_MOUNT: u32 = 0x34363a;
pub const BENCH_INK: u32 = 0xf0f0ec;
pub const BENCH_INK_BODY: u32 = 0xd6d6d0;
pub const BENCH_INK_DIM: u32 = 0x9d9d97;

// --- judge: neutral grey measuring station ---------------------------------
pub const JUDGE_GROUND: u32 = 0x1c1c1c;
pub const JUDGE_SUNK: u32 = 0x141414;
pub const JUDGE_RAISE: u32 = 0x262626;
pub const JUDGE_LINE: u32 = 0x3a3a3a;
pub const JUDGE_INK: u32 = 0xf0f0f0;
pub const JUDGE_INK_DIM: u32 = 0xa6a6a6;

// --- wax: one meaning each, never decoration -------------------------------
pub const WAX_WHITE: u32 = 0xf2efe6;
pub const WAX_WHITE_BRIGHT: u32 = 0xfffdf7;
pub const WAX_WHITE_DIM: u32 = 0x948b77;
pub const WAX_WHITE_TINT: u32 = 0x2f2a20;
pub const WAX_WHITE_INK: u32 = 0x14110d;
pub const WAX_AMBER: u32 = 0xe8a33d;
pub const WAX_AMBER_DIM: u32 = 0x8a6a2a;
pub const WAX_AMBER_TINT: u32 = 0x332817;
pub const WAX_RED: u32 = 0xd94a38;
pub const WAX_RED_DIM: u32 = 0x8f4033;
pub const WAX_RED_TINT: u32 = 0x3a1a16;
pub const WAX_RED_INK: u32 = 0xf0938a;
pub const WAX_RED_INK_SOFT: u32 = 0xf0c0b6;
pub const WAX_BLUE: u32 = 0x7ba0e8;
pub const WAX_BLUE_DIM: u32 = 0x3f5590;
pub const WAX_BLUE_TINT: u32 = 0x1c2540;

/// The 4px lattice. Every edge, row and rule lands on it.
pub const U: Pixels = px(4.0);
/// Default row height.
pub const ROW: Pixels = px(28.0);
/// Tight row height for the densest docks.
pub const ROW_TIGHT: Pixels = px(24.0);
/// Loose row height around controls.
pub const ROW_LOOSE: Pixels = px(32.0);
/// Every corner in the application.
pub const RADIUS: Pixels = px(3.0);

/// Text roles. Sizes and weights come from DESIGN.md's typography table.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Compressed Archivo at display scale: the loudest type, on the surface name.
    Display,
    /// Archivo at heading scale.
    Heading,
    /// Archivo at title scale: panel and section names.
    Title,
    /// Archivo body copy.
    Body,
    /// Archivo control labels.
    Control,
    /// Martian Mono label: uppercase edge codes and field labels.
    Label,
    /// Martian Mono edge code.
    EdgeCode,
    /// Martian Mono measured readout.
    Readout,
}

impl Face {
    pub fn family(self) -> SharedString {
        match self {
            Self::Display => SharedString::from("Archivo Compressed"),
            Self::Heading | Self::Title | Self::Body | Self::Control => {
                SharedString::from("Archivo")
            }
            Self::Label | Self::EdgeCode | Self::Readout => SharedString::from("Martian Mono"),
        }
    }

    pub fn size(self) -> Pixels {
        match self {
            Self::Display => px(17.0),
            Self::Heading => px(16.0),
            Self::Title => px(12.0),
            Self::Body => px(12.0),
            Self::Control => px(12.0),
            Self::Label | Self::EdgeCode => px(9.0),
            Self::Readout => px(10.0),
        }
    }

    pub fn weight(self) -> FontWeight {
        match self {
            Self::Display | Self::Heading | Self::Title => FontWeight::EXTRA_BOLD,
            Self::Body | Self::Readout => FontWeight::NORMAL,
            Self::Control => FontWeight::BOLD,
            Self::Label | Self::EdgeCode => FontWeight::MEDIUM,
        }
    }
}

/// The system's own printed codes are uppercase; user data and clickable labels
/// are not. GPUI has no text-transform, so the code is formed here.
pub fn code(text: &str) -> String {
    text.to_uppercase()
}

/// Handles and wires are marked in wax by the *kind* of data they carry, so a
/// wire says what it is before its label is read.
pub fn data_type_color(data_type: Option<&str>) -> u32 {
    match data_type.unwrap_or_default() {
        "core.Image" => WAX_WHITE,
        "core.Mask" => WAX_AMBER,
        "core.MaskSet" => 0xd98a2b,
        "core.LabelMap" => 0xc98f3a,
        "core.ConfidenceMap" => 0xf0c072,
        "core.DepthMap" => 0x8a6a2a,
        "core.RegionSet" => 0xb3701a,
        "core.Any" => 0xb8b0a0,
        "color.DisplayRGB" => WAX_BLUE,
        "color.SceneLinearRGB" => 0xa3bdf0,
        "value.Float" | "value.Integer" => 0xcdc5b4,
        "value.Boolean" | "value.Condition" => 0x9a927f,
        "value.String" => 0xb8b0a0,
        _ => 0x8d8676,
    }
}

/// A state stamp's ground and ink: one treatment, so a state is never decoded
/// twice, and the ink role rather than the dark mark value carries the text.
pub fn stamp(tone: Tone) -> (u32, u32) {
    match tone {
        Tone::Idle => (ROOM_RAISE, ROOM_INK_DIM),
        Tone::Fresh => (WAX_WHITE_TINT, WAX_WHITE),
        Tone::Held => (WAX_AMBER_TINT, WAX_AMBER),
        Tone::Failed => (WAX_RED_TINT, WAX_RED_INK),
        Tone::Working => (WAX_BLUE_TINT, WAX_BLUE),
    }
}

/// Register the two shipped faces. A failure is reported, never hidden: an
/// unresolved family silently falls back to a system face and the world is lost.
pub fn install_fonts(text_system: &TextSystem, cx: &App) -> Result<(), String> {
    let _ = cx;
    text_system
        .add_fonts(vec![
            include_bytes!("../assets/fonts/Archivo-Regular.ttf")
                .as_slice()
                .into(),
            include_bytes!("../assets/fonts/Archivo-Bold.ttf")
                .as_slice()
                .into(),
            include_bytes!("../assets/fonts/Archivo-ExtraBold.ttf")
                .as_slice()
                .into(),
            include_bytes!("../assets/fonts/ArchivoCompressed-ExtraBold.ttf")
                .as_slice()
                .into(),
            include_bytes!("../assets/fonts/MartianMono-Regular.ttf")
                .as_slice()
                .into(),
            include_bytes!("../assets/fonts/MartianMono-Medium.ttf")
                .as_slice()
                .into(),
        ])
        .map_err(|error| format!("design faces did not register: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance, computed from the tokens themselves so a palette
    /// edit cannot quietly drop an ink below its own cast's floor.
    fn luminance(token: u32) -> f32 {
        let channel = |shift: u32| {
            let value = ((token >> shift) & 0xff) as f32 / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }

    fn ratio(a: u32, b: u32) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn every_ink_survives_aa_on_the_worst_surface_of_its_own_cast() {
        // Room: the darkest and lightest handling surfaces both carry ink.
        for surface in [ROOM_GROUND, ROOM_SUNK, ROOM_STRIP, ROOM_PANEL, ROOM_RAISE] {
            assert!(
                ratio(ROOM_INK, surface) >= 4.5,
                "room ink on {surface:#08x}: {:.2}",
                ratio(ROOM_INK, surface)
            );
            assert!(
                ratio(ROOM_INK_DIM, surface) >= 4.5,
                "room dim ink on {surface:#08x}: {:.2}",
                ratio(ROOM_INK_DIM, surface)
            );
        }
        // Bench.
        for surface in [BENCH_GROUND, BENCH_RAISE, BENCH_HOVER, BENCH_SUNK] {
            assert!(
                ratio(BENCH_INK, surface) >= 4.5,
                "bench ink on {surface:#08x}: {:.2}",
                ratio(BENCH_INK, surface)
            );
            assert!(
                ratio(BENCH_INK_DIM, surface) >= 4.5,
                "bench dim ink on {surface:#08x}: {:.2}",
                ratio(BENCH_INK_DIM, surface)
            );
        }
        // Judge.
        for surface in [JUDGE_GROUND, JUDGE_SUNK, JUDGE_RAISE] {
            assert!(
                ratio(JUDGE_INK, surface) >= 4.5,
                "judge ink on {surface:#08x}: {:.2}",
                ratio(JUDGE_INK, surface)
            );
            assert!(
                ratio(JUDGE_INK_DIM, surface) >= 4.5,
                "judge dim ink on {surface:#08x}: {:.2}",
                ratio(JUDGE_INK_DIM, surface)
            );
        }
    }

    #[test]
    fn wax_ink_is_readable_on_its_own_tint_and_never_on_a_bare_wax() {
        // State text uses the ink role, never the mark value.
        assert!(ratio(WAX_RED_INK, WAX_RED_TINT) >= 4.5);
        assert!(ratio(WAX_AMBER, WAX_AMBER_TINT) >= 4.5);
        assert!(ratio(WAX_BLUE, WAX_BLUE_TINT) >= 4.5);
        assert!(ratio(WAX_WHITE_INK, WAX_WHITE) >= 4.5);
        // The dark mark values are not text colours.
        assert!(ratio(WAX_RED, ROOM_PANEL) < 4.5);
    }

    #[test]
    fn state_stamps_pair_an_ink_role_with_its_own_tint() {
        for (tone, ground, ink) in [
            (Tone::Idle, ROOM_RAISE, ROOM_INK_DIM),
            (Tone::Fresh, WAX_WHITE_TINT, WAX_WHITE),
            (Tone::Held, WAX_AMBER_TINT, WAX_AMBER),
            (Tone::Failed, WAX_RED_TINT, WAX_RED_INK),
            (Tone::Working, WAX_BLUE_TINT, WAX_BLUE),
        ] {
            assert_eq!(stamp(tone), (ground, ink));
            assert!(
                ratio(ink, ground) >= 4.5,
                "{tone:?}: {:.2}",
                ratio(ink, ground)
            );
        }
    }

    #[test]
    fn faces_carry_the_documented_scale_and_two_families() {
        assert_eq!(Face::Display.family().as_ref(), "Archivo Compressed");
        assert_eq!(Face::Body.family().as_ref(), "Archivo");
        assert_eq!(Face::Readout.family().as_ref(), "Martian Mono");
        assert_eq!(Face::Display.size(), px(17.0));
        assert_eq!(Face::Label.size(), px(9.0));
        assert_eq!(Face::Display.weight(), FontWeight::EXTRA_BOLD);
        assert_eq!(Face::Control.weight(), FontWeight::BOLD);
        assert_eq!(code("core.image-input"), "CORE.IMAGE-INPUT");
        // Four wax families, one tone step each; an unknown type is neutral.
        assert_eq!(data_type_color(Some("core.Image")), WAX_WHITE);
        assert_eq!(data_type_color(Some("core.Mask")), WAX_AMBER);
        assert_eq!(data_type_color(Some("color.DisplayRGB")), WAX_BLUE);
        assert_eq!(data_type_color(Some("value.Float")), 0xcdc5b4);
        assert_eq!(data_type_color(None), 0x8d8676);
        assert_eq!(RADIUS, px(3.0));
    }
}
