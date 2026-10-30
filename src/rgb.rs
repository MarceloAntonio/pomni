//! RGB fields recovered from the shipped OemDrv.exe; see docs/protocol.md.
//! These functions only edit buffers. The caller owns HID transactions.

pub const EFFECTS: &[(&str, u8, &str)] = &[
    ("static", 1, "Sempre ligado"),
    ("breathing", 2, "Respirar"),
    ("rainbow", 3, "Arco-íris"),
    ("fade", 4, "Desaparecer rapidamente"),
    ("rain", 5, "Gotas de chuva"),
    ("wheel", 6, "Roda arco-íris"),
    ("ripples", 7, "Ondulações brilhantes"),
    ("stars", 8, "Brilhos de estrela"),
    ("shadow", 9, "Desaparecimento em sombra"),
    ("snake", 10, "Cobra retrô"),
    ("neon", 11, "Fluxo de neon"),
    ("reactive", 12, "Reação"),
    ("sine", 13, "Onda senoidal"),
    ("scan", 14, "Digitalização contínua"),
    ("windmill", 15, "Moinho de vento"),
    ("waterfall", 16, "Cachoeira colorida"),
    ("bloom", 17, "Florescendo"),
    ("storm", 18, "Tempestade giratória"),
    ("off", 0, "Desligado"),
];

pub fn effect_name(hw: u8) -> &'static str {
    EFFECTS
        .iter()
        .find(|(_, n, _)| *n == hw)
        .map(|(s, _, _)| *s)
        .unwrap_or("custom/unknown")
}

/// The shipped driver's layout and the observed F75 wireless firmware differ.
/// See docs/brightness-evidence.md for the physical wheel captures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedLayout {
    Driver,
    F75Wireless,
}

impl LedLayout {
    pub fn for_connection(model: &str, wireless: bool) -> Self {
        if model == "F75" && wireless {
            Self::F75Wireless
        } else {
            Self::Driver
        }
    }

    fn brightness(self, led: &[u8; 128], effect_index: usize) -> u8 {
        match self {
            Self::Driver => led[0x3b + 2 * effect_index] >> 4,
            Self::F75Wireless => led[0x3a + 2 * effect_index],
        }
    }

    fn set_brightness(self, led: &mut [u8; 128], effect_index: usize, level: u8) {
        match self {
            Self::Driver => {
                let packed = &mut led[0x3b + 2 * effect_index];
                *packed = (level << 4) | (*packed & 0x0f);
            }
            Self::F75Wireless => led[0x3a + 2 * effect_index] = level,
        }
    }
}

fn max_brightness(model: &str) -> Option<u8> {
    match model {
        "F75" => Some(9),
        "F87" | "F99" => Some(4),
        _ => None,
    }
}

/// Read the active hardware brightness using the same layout as the writer.
pub fn brightness_percent(led: &[u8; 128], model: &str, layout: LedLayout) -> Option<u8> {
    if led[9] != 0 || led[10] == 0 || led[126..] != [0x5a, 0xa5] {
        return None;
    }
    let i = EFFECTS.iter().position(|(_, hw, _)| *hw == led[10])?;
    let max = max_brightness(model)?;
    let level = layout.brightness(led, i);
    (level <= max).then(|| ((u16::from(level) * 100 + u16::from(max) / 2) / u16::from(max)) as u8)
}

/// The last LedOpt column in the shipped model files enables color selection.
/// Hardware IDs are the FIRST LedOpt column, not the second effect/type column.
pub fn color_capable(hw: u8) -> bool {
    matches!(hw, 1 | 2 | 4..=14 | 18)
}

pub fn needs_palette(led: &[u8; 128], style: Option<&str>, color: Option<[u8; 3]>) -> bool {
    if color.is_some() {
        return true;
    }

    led[9] == 0
        && color_capable(led[10])
        && style.is_some_and(|name| {
            EFFECTS
                .iter()
                .any(|(s, hw, _)| *s == name && color_capable(*hw))
        })
}

/// Edit requested settings, carrying the active palette across compatible styles.
/// Built-in multicolor styles and styles without a recovered palette keep their
/// existing color behavior; a single visible color cannot be guaranteed for them.
/// Relative brightness advances at least one supported hardware step when nonzero.
pub fn apply(
    led: &mut [u8; 128],
    palette: Option<&mut [u8]>,
    model: &str,
    layout: LedLayout,
    style: Option<&str>,
    color: Option<[u8; 3]>,
    brightness: Option<crate::cli::Brightness>,
) -> Result<(), String> {
    use crate::cli::Brightness;
    let max_level =
        max_brightness(model).ok_or_else(|| format!("modelo desconhecido para RGB: {model}"))?;
    if led[126..128] != [0x5a, 0xa5] {
        return Err("bloco LED sem marcador 5a a5; configuração não alterada".into());
    }
    let i = match style {
        Some(name) => EFFECTS
            .iter()
            .position(|(s, _, _)| *s == name)
            .ok_or("estilo desconhecido; consulte effects")?,
        None => EFFECTS
            .iter()
            .position(|(_, h, _)| *h == led[10])
            .ok_or("estilo atual desconhecido; selecione --style explicitamente")?,
    };
    let (name, hw, _) = EFFECTS[i];
    let old_style = EFFECTS.iter().position(|(_, h, _)| *h == led[10]);
    let carry_color = style.is_some()
        && color.is_none()
        && led[9] == 0
        && color_capable(hw)
        && old_style.is_some_and(|old| color_capable(EFFECTS[old].1));
    let old_random = old_style.map(|old| led[0x3b + 2 * old] & 0x0f);
    if hw == 0 && (brightness.is_some() || color.is_some()) {
        return Err("o estilo off não aceita cor ou brilho; selecione --style static".into());
    }
    if style.is_none() && led[9] != 0 && (color.is_some() || brightness.is_some()) {
        return Err("modo custom atual não mapeado; selecione --style explicitamente".into());
    }
    let packed = 0x3b + 2 * i;
    let level = match brightness {
        Some(Brightness::Absolute(percent)) if percent <= 100 => {
            Some(((u16::from(percent) * u16::from(max_level) + 50) / 100) as u8)
        }
        Some(Brightness::Up(points) | Brightness::Down(points)) if points <= 100 => {
            let current = layout.brightness(led, i);
            if current > max_level {
                return Err("nível de brilho atual inválido; use --brightness 0..100".into());
            }
            let delta = (u16::from(points) * u16::from(max_level)).div_ceil(100) as u8;
            Some(if matches!(brightness, Some(Brightness::Up(_))) {
                current.saturating_add(delta).min(max_level)
            } else {
                current.saturating_sub(delta)
            })
        }
        Some(_) => return Err("brilho exige 0..100 ou alteração de até 100 pontos".into()),
        None => None,
    };
    if color.is_some() {
        // Last LedOpt column declares per-effect color support.
        if !color_capable(hw) {
            return Err(format!(
                "cor configurável não recuperada para {name}; use --style static"
            ));
        }
    }
    if color.is_some() || carry_color {
        match palette.as_deref() {
            None => return Err("cor exige leitura prévia da paleta RGB".into()),
            Some(p) if !p.ends_with(&[0x5a, 0xa5]) => {
                return Err("paleta RGB sem marcador 5a a5; configuração não alterada".into());
            }
            Some(p)
                if p.len() < 2 + (usize::from(hw) + 1) * 21
                    || (carry_color
                        && old_style.is_some_and(|old| {
                            p.len() < 2 + (usize::from(EFFECTS[old].1) + 1) * 21
                        })) =>
            {
                return Err("paleta RGB recebida não contém todas as cores necessárias".into());
            }
            Some(_) => (),
        }
    }
    // Every validation precedes mutation, preserving both buffers on any error.
    if style.is_some() {
        led[9] = 0;
        led[10] = hw;
    }
    if let Some(value) = level {
        layout.set_brightness(led, i, value);
    }
    if carry_color {
        let p = palette.expect("palette validated above");
        let source = usize::from(EFFECTS[old_style.expect("old style validated above")].1) * 21;
        let target = usize::from(hw) * 21;
        p.copy_within(source..source + 21, target);
        led[packed] = (led[packed] & 0xf0) | old_random.expect("old style validated above");
    } else if let Some(rgb) = color {
        let p = palette.expect("palette validated above");
        let offset = usize::from(hw) * 21;
        for triplet in p[offset..offset + 21].as_chunks_mut::<3>().0 {
            triplet.copy_from_slice(&rgb);
        }
        led[packed] &= 0xf0;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Brightness;

    #[test]
    fn f75_wireless_brightness_matches_physical_wheel_capture() {
        let before = *include_bytes!("../tests/fixtures/f75-wireless-neon-before.bin");
        let after = *include_bytes!("../tests/fixtures/f75-wireless-neon-after.bin");
        let layout = LedLayout::for_connection("F75", true);
        assert_eq!(before[10], 11); // neon, unchanged by the wheel
        assert_eq!(brightness_percent(&before, "F75", layout), Some(67));
        assert_eq!(brightness_percent(&after, "F75", layout), Some(22));
        for request in [Brightness::Absolute(22), Brightness::Down(40)] {
            let mut led = before;
            apply(&mut led, None, "F75", layout, None, None, Some(request)).unwrap();
            // Compare the entire independently captured block, including the
            // adjacent byte the old implementation incorrectly overwrote.
            assert_eq!(led, after);
        }
        let mut restored = after;
        apply(
            &mut restored,
            None,
            "F75",
            layout,
            None,
            None,
            Some(Brightness::Up(40)),
        )
        .unwrap();
        assert_eq!(restored, before);
    }

    #[test]
    fn f75_wireless_limits_and_effect_changes_preserve_neighboring_fields() {
        let captured = *include_bytes!("../tests/fixtures/f75-wireless-neon-before.bin");
        for (i, (name, hw, _)) in EFFECTS
            .iter()
            .enumerate()
            .filter(|(_, (_, hw, _))| *hw != 0)
        {
            let mut led = captured;
            // Palette transfer is deliberately avoided here; test brightness
            // and effect selection together without assuming any palette data.
            led[10] = 3; // rainbow has no transferable single-color palette
            apply(
                &mut led,
                None,
                "F75",
                LedLayout::F75Wireless,
                Some(name),
                None,
                Some(Brightness::Absolute(100)),
            )
            .unwrap();
            assert_eq!(led[10], *hw);
            assert_eq!(
                brightness_percent(&led, "F75", LedLayout::F75Wireless),
                Some(100)
            );
            for offset in 0..128 {
                if offset != 10 && offset != 0x3a + 2 * i {
                    assert_eq!(
                        led[offset], captured[offset],
                        "effect {name}, byte {offset}"
                    );
                }
            }
            apply(
                &mut led,
                None,
                "F75",
                LedLayout::F75Wireless,
                None,
                None,
                Some(Brightness::Up(10)),
            )
            .unwrap();
            assert_eq!(led[0x3a + 2 * i], 9);
            apply(
                &mut led,
                None,
                "F75",
                LedLayout::F75Wireless,
                None,
                None,
                Some(Brightness::Down(100)),
            )
            .unwrap();
            assert_eq!(led[0x3a + 2 * i], 0);
        }
        assert_eq!(LedLayout::for_connection("F75", false), LedLayout::Driver);
        for model in ["F87", "F99"] {
            assert_eq!(LedLayout::for_connection(model, true), LedLayout::Driver);
        }
    }

    #[test]
    fn hardware_ids_match_first_ledopt_column_not_effect_type() {
        // KB.ini LedOpt rows; loader 0x413227 stores first token at model+2fbe,
        // which command builder 0x495153 uses as the hardware effect ID.
        let fixture = "1,1;2,3;3,2;4,19;5,15;6,13;7,20;8,16;9,18;10,5;11,7;12,17;13,12;14,8;15,28;16,30;17,14;18,29;21,0;0,0";
        let rows: Vec<_> = fixture
            .split(';')
            .map(|row| row.split(',').next().unwrap().parse::<u8>().unwrap())
            .collect();
        for (i, (_, hardware, _)) in EFFECTS[..18].iter().enumerate() {
            assert_eq!(*hardware, rows[i]);
        }
        assert_eq!(EFFECTS[18].1, rows[19]); // off, custom hardware21 is excluded.
        assert_eq!(effect_name(2), "breathing");
        assert_eq!(effect_name(16), "waterfall");
        assert_eq!(effect_name(30), "custom/unknown");
        for hw in 0..=21 {
            assert_eq!(color_capable(hw), matches!(hw, 1 | 2 | 4..=14 | 18));
        }
    }

    #[test]
    fn waterfall_change_requires_no_palette() {
        let mut led = led_fixture();
        assert!(!needs_palette(&led, Some("waterfall"), None));
        assert!(needs_palette(&led, Some("breathing"), None));
        let previous = led;
        apply(
            &mut led,
            None,
            "F75",
            LedLayout::Driver,
            Some("waterfall"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(led[10], 16);
        for i in 0..128 {
            if i != 9 && i != 10 {
                assert_eq!(led[i], previous[i]);
            }
        }
    }

    #[test]
    fn short_wireless_palette_is_edited_without_expansion() {
        let mut led = led_fixture();
        let mut palette = vec![0x33; 480];
        palette[478..].copy_from_slice(&[0x5a, 0xa5]);
        let previous = palette.clone();
        apply(
            &mut led,
            Some(&mut palette),
            "F75",
            LedLayout::Driver,
            None,
            Some([0x4c, 0xb5, 0xe8]),
            None,
        )
        .unwrap();
        assert_eq!(palette.len(), 480);
        assert_eq!(&palette[..21], &previous[..21]);
        assert_eq!(&palette[42..], &previous[42..]);
        for rgb in palette[21..42].as_chunks::<3>().0 {
            assert_eq!(*rgb, [0x4c, 0xb5, 0xe8]);
        }
    }

    fn led_fixture() -> [u8; 128] {
        let mut data = [0; 128];
        data[10] = 1;
        data[59] = 0x47;
        data[61] = 0x27;
        data[126..].copy_from_slice(&[0x5a, 0xa5]);
        data
    }
    fn palette_fixture() -> [u8; 512] {
        let mut data = std::array::from_fn(|i| (i % 251) as u8);
        data[510..].copy_from_slice(&[0x5a, 0xa5]);
        data
    }
    #[test]
    fn color_only_preserves_active_style_and_unrelated_bytes() {
        let mut led = led_fixture();
        let before = led;
        let mut palette = palette_fixture();
        let before_palette = palette;
        apply(
            &mut led,
            Some(&mut palette),
            "F75",
            LedLayout::Driver,
            None,
            Some([0x12, 0x34, 0xab]),
            None,
        )
        .unwrap();
        assert_eq!(led[10], before[10]);
        assert_eq!(led[59], 0x40);
        for i in 0..128 {
            if i != 59 {
                assert_eq!(led[i], before[i]);
            }
        }
        assert_eq!(&palette[..21], &before_palette[..21]);
        assert_eq!(&palette[42..], &before_palette[42..]);
        for rgb in palette[21..42].as_chunks::<3>().0 {
            assert_eq!(*rgb, [0x12, 0x34, 0xab]);
        }
    }
    #[test]
    fn unsupported_palette_style_preserves_all_stored_palettes() {
        let mut led = led_fixture();
        let before = led;
        let mut palette = palette_fixture();
        let before_palette = palette;
        apply(
            &mut led,
            Some(&mut palette),
            "F75",
            LedLayout::Driver,
            Some("waterfall"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(led[10], 16);
        assert_eq!(palette, before_palette);
        for i in 0..128 {
            if ![9, 10].contains(&i) {
                assert_eq!(led[i], before[i]);
            }
        }
    }
    #[test]
    fn style_only_carries_active_colors_and_random_flag_to_target() {
        let mut led = led_fixture();
        led[59] = 0x40;
        led[61] = 0x27;
        let before = led;
        let mut palette = palette_fixture();
        let before_palette = palette;
        apply(
            &mut led,
            Some(&mut palette),
            "F75",
            LedLayout::Driver,
            Some("breathing"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(led[10], 2);
        assert_eq!(led[61], 0x20);
        assert_eq!(&palette[42..63], &before_palette[21..42]);
        assert_eq!(&palette[..42], &before_palette[..42]);
        assert_eq!(&palette[63..], &before_palette[63..]);
        for i in 0..128 {
            if ![9, 10, 61].contains(&i) {
                assert_eq!(led[i], before[i]);
            }
        }
        // A second style change carries the random flag as well as RGB bytes.
        led[61] |= 7;
        apply(
            &mut led,
            Some(&mut palette),
            "F75",
            LedLayout::Driver,
            Some("static"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(led[59], 0x47);
    }
    #[test]
    fn brightness_quantization_and_relative_steps_follow_model() {
        for (model, percent, expected) in [
            ("F75", 0, 0),
            ("F75", 50, 5),
            ("F75", 100, 9),
            ("F87", 50, 2),
            ("F99", 100, 4),
        ] {
            let mut led = led_fixture();
            apply(
                &mut led,
                None,
                model,
                LedLayout::Driver,
                None,
                None,
                Some(Brightness::Absolute(percent)),
            )
            .unwrap();
            assert_eq!(led[59] >> 4, expected);
            assert_eq!(led[59] & 15, 7);
        }
        let mut led = led_fixture();
        apply(
            &mut led,
            None,
            "F87",
            LedLayout::Driver,
            None,
            None,
            Some(Brightness::Down(10)),
        )
        .unwrap();
        assert_eq!(led[59] >> 4, 3);
        apply(
            &mut led,
            None,
            "F87",
            LedLayout::Driver,
            None,
            None,
            Some(Brightness::Up(100)),
        )
        .unwrap();
        assert_eq!(led[59] >> 4, 4);
        apply(
            &mut led,
            None,
            "F87",
            LedLayout::Driver,
            None,
            None,
            Some(Brightness::Down(100)),
        )
        .unwrap();
        assert_eq!(led[59] >> 4, 0);
    }
    #[test]
    fn combined_color_style_and_brightness_edit_target_only() {
        let mut led = led_fixture();
        let mut palette = palette_fixture();
        let before = palette;
        apply(
            &mut led,
            Some(&mut palette),
            "F87",
            LedLayout::Driver,
            Some("breathing"),
            Some([1, 2, 3]),
            Some(Brightness::Absolute(100)),
        )
        .unwrap();
        assert_eq!(led[10], 2);
        assert_eq!(led[61], 0x40);
        assert_eq!(&palette[..42], &before[..42]);
        assert_eq!(&palette[63..], &before[63..]);
        for rgb in palette[42..63].as_chunks::<3>().0 {
            assert_eq!(*rgb, [1, 2, 3]);
        }
    }
    #[test]
    fn unsupported_colors_and_bad_markers_never_mutate() {
        for style in ["off", "waterfall", "rainbow", "bloom"] {
            let mut led = led_fixture();
            let before = led;
            let mut palette = palette_fixture();
            let before_palette = palette;
            assert!(
                apply(
                    &mut led,
                    Some(&mut palette),
                    "F75",
                    LedLayout::Driver,
                    Some(style),
                    Some([1, 2, 3]),
                    None
                )
                .is_err()
            );
            assert_eq!(led, before);
            assert_eq!(palette, before_palette);
        }
        let mut led = led_fixture();
        let before = led;
        let mut bad_palette = [0; 512];
        assert!(
            apply(
                &mut led,
                Some(&mut bad_palette),
                "F75",
                LedLayout::Driver,
                None,
                Some([1, 2, 3]),
                None
            )
            .is_err()
        );
        assert_eq!(led, before);
        assert_eq!(bad_palette, [0; 512]);
        led[127] = 0;
        let before = led;
        assert!(
            apply(
                &mut led,
                None,
                "F75",
                LedLayout::Driver,
                Some("static"),
                None,
                None
            )
            .is_err()
        );
        assert_eq!(led, before);
    }
}
