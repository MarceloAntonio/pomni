use std::path::PathBuf;

pub const HELP: &str = "aula-cli — AULA F Series no terminal (Linux)\n\nUso:\n  aula-cli info [--battery] [--mode]\n  aula-cli set [--color RRGGBB] [--style NOME] [--brightness VALOR] [--dry-run]\n  aula-cli set --brightness-up\n  aula-cli set --brightness-down\n  aula-cli effects\n  aula-cli list [--all]\n  aula-cli models\n\nO teclado é detectado automaticamente. Cor e estilo omitidos são preservados.\nBrilho: percentual 0..100 ou alteração +10/-10; up/down alteram 10 pontos.\nUse --color ff0000 ou --color '#ff0000'; sem aspas, # inicia comentário no shell.\n--dry-run consulta o estado e mostra a alteração sem gravar configurações.\n";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Brightness {
    Absolute(u8),
    Up(u8),
    Down(u8),
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    List {
        all: bool,
    },
    Info {
        battery: bool,
        mode: bool,
    },
    Models,
    Effects,
    Set {
        color: Option<[u8; 3]>,
        style: Option<String>,
        brightness: Option<Brightness>,
        dry_run: bool,
    },
}

pub fn parse(args: Vec<String>) -> Result<Command, String> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] | ["help"] | ["--help"] | ["-h"] => return Ok(Command::Help),
        ["list"] => return Ok(Command::List { all: false }),
        ["list", "--all"] => return Ok(Command::List { all: true }),
        ["models"] => return Ok(Command::Models),
        ["effects"] => return Ok(Command::Effects),
        _ => (),
    }
    match words.as_slice() {
        ["info", rest @ ..] => {
            let (mut battery, mut mode) = (false, false);
            for option in rest {
                match *option {
                    "--battery" if !battery => battery = true,
                    "--mode" if !mode => mode = true,
                    s => return Err(format!("opção inválida ou repetida em info: {s}")),
                }
            }
            Ok(Command::Info { battery, mode })
        }
        ["set", rest @ ..] => {
            let (mut color, mut style, mut brightness, mut dry_run) = (None, None, None, false);
            let mut options = rest.iter().copied();
            while let Some(option) = options.next() {
                match option {
                    "--color" if color.is_none() => {
                        let value = options.next().ok_or("--color exige RRGGBB; use --color ff0000 ou --color '#ff0000' (com aspas)")?;
                        color = Some(parse_color(value)?);
                    }
                    "--style" if style.is_none() => {
                        let value = options
                            .next()
                            .ok_or("--style exige um nome; consulte effects")?;
                        if !crate::rgb::EFFECTS
                            .iter()
                            .any(|(name, _, _)| *name == value)
                        {
                            return Err(format!("estilo desconhecido: {value}; consulte effects"));
                        }
                        style = Some(value.to_string());
                    }
                    "--brightness" if brightness.is_none() => {
                        let value = options.next().ok_or("--brightness exige 0..100 ou +N/-N")?;
                        brightness = Some(parse_brightness(value)?);
                    }
                    "--brightness-up" if brightness.is_none() => {
                        brightness = Some(Brightness::Up(10))
                    }
                    "--brightness-down" if brightness.is_none() => {
                        brightness = Some(Brightness::Down(10))
                    }
                    "--dry-run" if !dry_run => dry_run = true,
                    s => return Err(format!("opção inválida ou repetida em set: {s}")),
                }
            }
            if color.is_none() && style.is_none() && brightness.is_none() {
                return Err("set exige --color, --style ou uma opção de brilho".into());
            }
            Ok(Command::Set {
                color,
                style,
                brightness,
                dry_run,
            })
        }
        _ => Err("comando inválido; consulte --help".into()),
    }
}

fn parse_color(value: &str) -> Result<[u8; 3], String> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("cor inválida; use RRGGBB ou '#RRGGBB' com aspas".into());
    }
    let n = u32::from_str_radix(hex, 16).map_err(|_| "cor inválida")?;
    Ok([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

fn parse_brightness(value: &str) -> Result<Brightness, String> {
    let (sign, digits) = if let Some(digits) = value.strip_prefix('+') {
        (1, digits)
    } else if let Some(digits) = value.strip_prefix('-') {
        (-1, digits)
    } else {
        (0, value)
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err("brilho exige 0..100 ou alteração +N/-N de até 100 pontos".into());
    }
    let n = digits
        .parse::<u8>()
        .ok()
        .filter(|n| *n <= 100)
        .ok_or("brilho exige 0..100 ou alteração +N/-N de até 100 pontos")?;
    Ok(match sign {
        1 => Brightness::Up(n),
        -1 => Brightness::Down(n),
        _ => Brightness::Absolute(n),
    })
}

// Used internally to validate detected hidraw paths, not exposed as a CLI option.
pub fn device_path(path: &str) -> Result<PathBuf, String> {
    let name = path
        .strip_prefix("/dev/hidraw")
        .ok_or("use um caminho /dev/hidrawN")?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
        return Err("use um caminho /dev/hidrawN".into());
    }
    Ok(path.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse_args(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|s| s.to_string()).collect())
    }
    #[test]
    fn color_and_style_are_independent_optional_updates() {
        assert_eq!(
            parse_args(&["set", "--color", "#12AB34"]).unwrap(),
            Command::Set {
                color: Some([0x12, 0xab, 0x34]),
                style: None,
                brightness: None,
                dry_run: false
            }
        );
        assert_eq!(
            parse_args(&["set", "--style", "breathing", "--dry-run"]).unwrap(),
            Command::Set {
                color: None,
                style: Some("breathing".into()),
                brightness: None,
                dry_run: true
            }
        );
    }
    #[test]
    fn brightness_relative_and_absolute_are_distinct() {
        for (option, value) in [
            ("50", Brightness::Absolute(50)),
            ("+10", Brightness::Up(10)),
            ("-10", Brightness::Down(10)),
        ] {
            assert_eq!(
                parse_args(&["set", "--brightness", option]).unwrap(),
                Command::Set {
                    color: None,
                    style: None,
                    brightness: Some(value),
                    dry_run: false
                }
            );
        }
        for (option, value) in [
            ("--brightness-up", Brightness::Up(10)),
            ("--brightness-down", Brightness::Down(10)),
        ] {
            assert_eq!(
                parse_args(&["set", option]).unwrap(),
                Command::Set {
                    color: None,
                    style: None,
                    brightness: Some(value),
                    dry_run: false
                }
            );
        }
    }
    #[test]
    fn info_selectors_can_be_combined_and_duplicates_fail() {
        assert_eq!(
            parse_args(&["info"]).unwrap(),
            Command::Info {
                battery: false,
                mode: false
            }
        );
        assert_eq!(
            parse_args(&["info", "--mode", "--battery"]).unwrap(),
            Command::Info {
                battery: true,
                mode: true
            }
        );
        assert!(parse_args(&["info", "--battery", "--battery"]).is_err());
    }
    #[test]
    fn malformed_arguments_fail_before_hardware_access() {
        for args in [
            vec!["set"],
            vec!["set", "--dry-run"],
            vec!["set", "--color"],
            vec!["set", "--color", "##ff0000"],
            vec!["set", "--color", "zz0000"],
            vec!["set", "--style", "missing"],
            vec!["set", "--brightness", "101"],
            vec!["set", "--brightness", "--10"],
            vec!["set", "--brightness-up", "--brightness-down"],
            vec!["set", "--style", "static", "--style", "rainbow"],
            vec!["info", "--device", "/dev/hidraw5"],
        ] {
            assert!(parse_args(&args).is_err(), "unexpected success: {args:?}");
        }
        assert!(
            parse_args(&["set", "--color"])
                .unwrap_err()
                .contains("aspas")
        );
        assert!(device_path("/dev/hidraw1/../../sda").is_err());
    }
}
