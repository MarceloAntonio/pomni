mod cli;
mod descriptor;
mod device;
mod protocol;
mod rgb;
mod transport;

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
            let d = device::detect()?;
            let h = protocol::open(&d)?;
            let model = identify(&h)?;
            let mut led = protocol::read_led(&h)?;
            let old_led = led;
            let mut palette = if rgb::needs_palette(&led, style.as_deref(), color) {
                Some(protocol::read_palette(&h)?)
            } else {
                None
            };
            let old_palette = palette.clone();
            rgb::apply(
                &mut led,
                palette.as_deref_mut(),
                model,
                style.as_deref(),
                color,
                brightness,
            )?;
            println!(
                "Teclado: {model}; modo: {}; estilo: {}",
                d.mode(),
                rgb::effect_name(led[10])
            );
            if dry_run {
                println!("Prévia pronta; nenhuma configuração gravada.");
                return Ok(());
            }
            if palette != old_palette {
                let p = palette.as_ref().ok_or("paleta não disponível")?;
                protocol::set_palette(&h, p)?;
                if protocol::read_palette(&h)? != *p {
                    return Err(
                        "paleta gravada, mas releitura divergente; estilo não gravado".into(),
                    );
                }
            }
            if palette != old_palette {
                // Original palette -> LED flow waits 100 ms before applying LED.
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            if led != old_led {
                protocol::set_led(&h, &led)?;
                if protocol::read_led(&h)? != led {
                    return Err("configuração enviada, mas a releitura divergiu".into());
                }
            }
            println!("Configuração confirmada por releitura.");
        }
    }
    Ok(())
}

fn identify(h: &transport::Hidraw) -> Result<&'static str, String> {
    let psd = protocol::query_psd(h)?;
    protocol::model(&psd)
        .ok_or_else(|| format!("não reconheceu o teclado: identificação {psd:02x?}"))
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
