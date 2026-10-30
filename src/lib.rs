pub mod cli;
pub mod descriptor;
pub mod device;
pub mod protocol;
pub mod rgb;
pub mod transport;

pub fn apply_settings(
    color: Option<[u8; 3]>,
    style: Option<String>,
    brightness: Option<cli::Brightness>,
    dry_run: bool,
) -> Result<String, String> {
    let d = device::detect()?;
    let h = protocol::open(&d)?;
    let model = identify(&h)?;
    let mut led = protocol::read_led(&h)?;
    let layout = rgb::LedLayout::for_connection(model, h.wireless());
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
        layout,
        style.as_deref(),
        color,
        brightness,
    )?;
    let mut summary = format!(
        "Teclado: {model}; modo: {}; estilo: {}",
        d.mode(),
        rgb::effect_name(led[10])
    );
    if brightness.is_some() {
        let before = rgb::brightness_percent(&old_led, model, layout)
            .map_or_else(|| "indisponível".into(), |p| format!("{p}%"));
        let after = rgb::brightness_percent(&led, model, layout)
            .map_or_else(|| "indisponível".into(), |p| format!("{p}%"));
        summary.push_str(&format!("\nBrilho: {before} → {after}"));
    }
    if dry_run {
        return Ok(format!(
            "{summary}\nPrévia pronta; nenhuma configuração gravada."
        ));
    }
    if palette != old_palette {
        let p = palette.as_ref().ok_or("paleta não disponível")?;
        protocol::set_palette(&h, p)?;
        if protocol::read_palette(&h)? != *p {
            return Err("paleta gravada, mas releitura divergente; estilo não gravado".into());
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
    Ok(format!("{summary}\nConfiguração confirmada por releitura."))
}

pub fn identify(h: &transport::Hidraw) -> Result<&'static str, String> {
    let psd = protocol::query_psd(h)?;
    protocol::model(&psd)
        .ok_or_else(|| format!("não reconheceu o teclado: identificação {psd:02x?}"))
}
