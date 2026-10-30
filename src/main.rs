use aula_cli::{cli, descriptor, device, identify, protocol, rgb};

fn run() -> Result<(), String> {
    match cli::parse(std::env::args().skip(1).collect())? {
        cli::Command::Help => print!("{}", cli::HELP),
        cli::Command::Effects => {
            for (name, _, label) in rgb::EFFECTS {
                println!("{name:16} {label}");
            }
        }
        cli::Command::Models => println!(
            "AULA F75 / F87 / F99; modelo confirmado por PSD antes de gravar configurações."
        ),
        cli::Command::List { all } => {
            for d in device::enumerate()
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|d| all || d.is_aula())
            {
                println!(
                    "{} {:04x}:{:04x} {} ({})",
                    d.path.display(),
                    d.vid,
                    d.pid,
                    d.name,
                    d.mode()
                );
                if let Ok(desc) = d.descriptor() {
                    println!("  {}\n  Conexão: {}", descriptor::summary(&desc), d.phys);
                }
            }
        }
        cli::Command::Info { battery, mode } => {
            let d = device::detect()?;
            if mode || !battery {
                println!("Modo: {}", d.mode());
            }
            if !mode && !battery {
                println!("Dispositivo: {} ({})", d.name, d.path.display());
            }
            if battery || !mode {
                let capacity = match d.battery()? {
                    Some(p) => Some(p),
                    None if d.supports_configuration() => {
                        let h = protocol::open(&d)?;
                        identify(&h)?;
                        protocol::battery(&h)?
                    }
                    None => None,
                };
                match capacity {
                    Some(percent) => println!("Bateria: {}", battery_bar(percent)),
                    None => println!(
                        "Bateria: [??????????] indisponível nesta conexão (o teclado não expôs o nível)."
                    ),
                }
            }
        }
        cli::Command::Set {
            color,
            style,
            brightness,
            dry_run,
        } => {
            println!(
                "{}",
                aula_cli::apply_settings(color, style, brightness, dry_run)?
            );
        }
    }
    Ok(())
}

fn battery_bar(percent: u8) -> String {
    let filled = (usize::from(percent) + 5) / 10;
    format!(
        "[{}{}] {percent}%",
        "█".repeat(filled),
        "░".repeat(10 - filled)
    )
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Erro: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn battery_bar_has_exact_ten_cells_and_real_percent() {
        assert_eq!(battery_bar(0), "[░░░░░░░░░░] 0%");
        assert_eq!(battery_bar(54), "[█████░░░░░] 54%");
        assert_eq!(battery_bar(100), "[██████████] 100%");
    }
}
