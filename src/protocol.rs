//! Descriptor-selected AULA wired feature and wireless report19 protocols.
//! Wired Feature 6/9 and wireless Input/Output report 19 backends.
use crate::transport::Hidraw;
use std::time::Instant;
use std::{io, time::Duration};

const REPORT: u8 = 0x13;
const PACKET: usize = 20;
const CHUNK: usize = 14;
// The vendor implementation waits about one second per read and resends the
// request up to five times when the receiver does not answer.
const TIMEOUT: Duration = Duration::from_secs(1);
const READ_ATTEMPTS: usize = 5;

trait Transport {
    fn send(&self, packet: &[u8]) -> io::Result<usize>;
    fn receive(&self, packet: &mut [u8], timeout: Duration) -> io::Result<usize>;
}

impl Transport for Hidraw {
    fn send(&self, packet: &[u8]) -> io::Result<usize> {
        self.write_output(packet)
    }
    fn receive(&self, packet: &mut [u8], timeout: Duration) -> io::Result<usize> {
        self.read_input(packet, timeout)
    }
}

fn packet(command: u8, count: u8, index: u8, layer: u8, payload: &[u8]) -> [u8; PACKET] {
    assert!(payload.len() <= CHUNK);
    let mut bytes = [0; PACKET];
    bytes[0] = REPORT;
    bytes[1] = command;
    bytes[2] = count;
    bytes[3] = index;
    bytes[4] = (layer << 4) | payload.len() as u8;
    bytes[5..5 + payload.len()].copy_from_slice(payload);
    bytes[19] = bytes[..19].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    bytes
}

fn send(t: &impl Transport, bytes: &[u8; PACKET]) -> Result<(), String> {
    trace("TX", bytes);
    let n = t
        .send(bytes)
        .map_err(|e| format!("falha ao enviar relatório HID: {e}"))?;
    if n != PACKET {
        return Err(format!("envio HID incompleto: {n}/{PACKET} bytes"));
    }
    Ok(())
}

// The original receive routines do not verify a checksum. We validate framing,
// command, status, declared lengths, and fragment identity instead of guessing
// a receive checksum rule from the transmit checksum.
#[derive(Debug)]
enum ReceiveError {
    Timeout,
    Failure(String),
}
impl std::fmt::Display for ReceiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => f.write_str("tempo esgotado aguardando resposta do teclado"),
            Self::Failure(s) => f.write_str(s),
        }
    }
}
impl From<String> for ReceiveError {
    fn from(s: String) -> Self {
        Self::Failure(s)
    }
}
impl From<&str> for ReceiveError {
    fn from(s: &str) -> Self {
        Self::Failure(s.into())
    }
}

fn tracing_enabled() -> bool {
    std::env::var("POMNI_TRACE").as_deref() == Ok("1")
        || std::env::var("AULA_TRACE").as_deref() == Ok("1")
}

fn trace(label: &str, bytes: &[u8]) {
    if tracing_enabled() && bytes.first() == Some(&REPORT) {
        eprintln!("POMNI {label}: {:02x?}", bytes);
    }
}

fn stage(label: &str) {
    if tracing_enabled() {
        eprintln!("POMNI etapa: {label}");
    }
}

fn receive(
    t: &impl Transport,
    command: u8,
    deadline: Instant,
) -> Result<[u8; PACKET], ReceiveError> {
    for _ in 0..64 {
        let left = deadline
            .checked_duration_since(Instant::now())
            .ok_or(ReceiveError::Timeout)?;
        let mut bytes = [0; PACKET];
        let n = t.receive(&mut bytes, left).map_err(|e| {
            if e.kind() == io::ErrorKind::TimedOut {
                ReceiveError::Timeout
            } else {
                ReceiveError::Failure(format!("falha ao receber relatório HID: {e}"))
            }
        })?;
        if n == 0 {
            return Err("teclado desconectado durante a leitura".into());
        }
        if bytes[0] == 0xff {
            return Err("receptor informou desconexão do teclado".into());
        }
        if bytes[0] != REPORT {
            continue;
        }
        trace("RX", &bytes[..n.min(PACKET)]);
        if n != PACKET {
            return Err(format!("resposta HID truncada: {n}/{PACKET} bytes").into());
        }
        if bytes[1] & 0x7f != command {
            continue;
        }
        if bytes[1] & 0x80 != 0 || bytes[2] & 0x80 != 0 {
            return Err(format!("teclado rejeitou comando 0x{command:02x}").into());
        }
        return Ok(bytes);
    }
    Err("muitos relatórios alheios à operação; resposta esperada não recebida".into())
}

fn read_data<const N: usize>(t: &impl Transport, command: u8) -> Result<[u8; N], String> {
    let count = N.div_ceil(CHUNK);
    if count == 0 || count > 127 {
        return Err("tamanho de leitura inválido".into());
    }
    let mut last_error = None;
    for _attempt in 0..READ_ATTEMPTS {
        send(t, &packet(command, 1, 0, 0, &[]))?;
        let deadline = Instant::now() + TIMEOUT;
        let mut data = [0; N];
        let mut seen = vec![false; count];
        let mut remaining = count;
        // Track fragment identity, as the driver's getter does; place data by
        // index so reordering cannot corrupt a read-modify-write operation.
        let mut retry = false;
        for _ in 0..128 {
            let bytes = match receive(t, command, deadline) {
                Ok(bytes) => bytes,
                Err(ReceiveError::Timeout) => {
                    last_error = Some("tempo esgotado aguardando resposta do teclado".to_string());
                    retry = true;
                    break;
                }
                Err(error) => return Err(error.to_string()),
            };
            let declared_count = usize::from(bytes[2] & 0x7f);
            let index = usize::from(bytes[3]);
            let len = usize::from(bytes[4] & 0x0f);
            if declared_count != count || index >= count {
                return Err(format!(
                    "fragmentação inválida: índice {index}, total {declared_count}, esperado {count}"
                ));
            }
            let start = index * CHUNK;
            let expected_len = CHUNK.min(N - start);
            // The vendor parser clamps padded final payloads to the requested
            // size. Permit padding, never gaps or payloads over 14 bytes.
            if len < expected_len || len > CHUNK {
                return Err(format!(
                    "fragmento {index} tem {len} bytes; esperado entre {expected_len} e {CHUNK}"
                ));
            }
            if seen[index] {
                if data[start..start + expected_len] != bytes[5..5 + expected_len] {
                    return Err(format!(
                        "fragmento duplicado {index} contém dados diferentes"
                    ));
                }
                continue;
            }
            data[start..start + expected_len].copy_from_slice(&bytes[5..5 + expected_len]);
            seen[index] = true;
            remaining -= 1;
            if remaining == 0 {
                return Ok(data);
            }
        }
        if !retry {
            return Err("resposta incompleta: excesso de fragmentos duplicados".into());
        }
    }
    Err(format!(
        "sem resposta após {READ_ATTEMPTS} tentativas; confirme que a interface correta do receptor está selecionada e tente novamente ({})",
        last_error.unwrap_or_else(|| "timeout".into())
    ))
}

fn discover_wireless(t: &impl Transport) -> Result<[u8; 6], String> {
    stage("wireless: consulta de estado (07)");
    let state = read_data::<1>(t, 7);
    if matches!(state, Ok([0])) {
        stage("wireless: aguardando notificação de conexão (0a)");
        wait_connection(t, Duration::from_secs(5))
            .map_err(|e| format!("descoberta wireless: receptor respondeu desconectado; {e}. Acorde o teclado em modo 2,4 GHz e tente novamente"))?;
        std::thread::sleep(Duration::from_millis(200));
    }
    // The original watcher proceeds to PSD when state7 fails; it does not
    // substitute a feature-report command or assume the keyboard is absent.
    stage("wireless: consulta de identificação PSD (05)");
    std::thread::sleep(Duration::from_millis(20));
    read_data(t, 5).map_err(|e| match state {
        Err(state_error) => format!("descoberta wireless: consulta de estado07 falhou ({state_error}); consulta PSD05 também falhou ({e})"),
        _ => format!("descoberta wireless: consulta PSD05 falhou ({e})"),
    })
}

fn wait_connection(t: &impl Transport, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    for _ in 0..128 {
        let left = deadline
            .checked_duration_since(Instant::now())
            .ok_or("tempo esgotado aguardando notificação de conexão")?;
        let mut bytes = [0; PACKET];
        let n = t
            .receive(&mut bytes, left)
            .map_err(|e| format!("notificação de conexão não recebida: {e}"))?;
        if bytes.first() == Some(&REPORT) {
            trace("RX notificação", &bytes[..n.min(PACKET)]);
            if n != PACKET {
                return Err("notificação wireless truncada".into());
            }
            if bytes[1] == 0x0a && bytes[6] == 1 {
                return Ok(());
            }
        }
        if n == 0 || bytes[0] == 0xff {
            return Err("receptor desconectado".into());
        }
    }
    Err("notificação de conexão ausente após muitos relatórios".into())
}

/// Query six model-identification bytes through the receiver's output channel.
pub fn query_psd(hid: &Hidraw) -> Result<[u8; 6], String> {
    if hid.wireless() {
        return discover_wireless(hid);
    }
    feature_read(hid, feature_id(hid)?, 0x82, 1)
}

pub fn model(psd: &[u8; 6]) -> Option<&'static str> {
    match psd {
        [3, 0, 0, 0, 3, 0xc1] => Some("F87"),
        [3, 0, 0, 0, 3, 0xc2] => Some("F75"),
        [3, 0, 0, 0, 3, 0xc3] => Some("F99"),
        _ => None,
    }
}

pub fn read_led(hid: &Hidraw) -> Result<[u8; 128], String> {
    if hid.wireless() {
        return read_data(hid, 0x44).map_err(|e| format!("leitura RGB wireless: {e}"));
    }
    feature_read(hid, feature_id(hid)?, 0x84, 0)
}

pub fn read_palette(hid: &Hidraw) -> Result<Vec<u8>, String> {
    if hid.wireless() {
        let psd = query_psd(hid)?;
        let received = read_variable(hid, 0x49, 512)
            .map_err(|e| format!("leitura da paleta wireless: {e}"))?;
        return palette_write_image(&received, &psd);
    }
    feature_read::<512>(hid, feature_id(hid)?, 0x8a, 0).map(Vec::from)
}

// Vendor CRC=1 writer: 512-byte zero-filled image, marker at 510..512
// (0x49c9ab..0x49ca33). The user's captured getter returns 490 bytes,
// including its PSD at 483..489. Keep all received bytes, then add only
// the unreturned write framing. This does not pad an incomplete read.
fn palette_write_image(received: &[u8], psd: &[u8; 6]) -> Result<Vec<u8>, String> {
    if model(psd).is_none() {
        return Err("modelo desconhecido ao preparar paleta wireless".into());
    }
    match received.len() {
        512 => {
            validate_marker(received)?;
            Ok(received.to_vec())
        }
        490 if received[483..489] == *psd => {
            let mut image = vec![0; 512];
            image[..490].copy_from_slice(received);
            image[510..].copy_from_slice(&[0x5a, 0xa5]);
            stage(
                "paleta wireless: resposta completa de 490 bytes; imagem de escrita de 512 bytes",
            );
            Ok(image)
        }
        n => Err(format!(
            "formato de paleta wireless não reconhecido ({n} bytes ou identificação final divergente); nenhuma gravação enviada"
        )),
    }
}

// Unlike PSD/LED reads, palette size differs between firmware transports.
// Preserve the exact lengths declared by the device, never pad missing bytes.
fn read_variable(t: &impl Transport, command: u8, max_bytes: usize) -> Result<Vec<u8>, String> {
    for _ in 0..READ_ATTEMPTS {
        send(t, &packet(command, 1, 0, 0, &[]))?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut fragments: Vec<Option<Vec<u8>>> = Vec::new();
        let mut retry = false;
        for _ in 0..128 {
            let bytes = match receive(t, command, deadline) {
                Ok(bytes) => bytes,
                Err(ReceiveError::Timeout) => {
                    retry = true;
                    break;
                }
                Err(error) => return Err(error.to_string()),
            };
            let count = usize::from(bytes[2]);
            let index = usize::from(bytes[3]);
            let len = usize::from(bytes[4] & 15);
            if count == 0
                || count > max_bytes.div_ceil(CHUNK)
                || index >= count
                || len == 0
                || len > CHUNK
                || (index + 1 < count && len != CHUNK)
                || index * CHUNK + len > max_bytes
            {
                return Err("fragmento de paleta com tamanho ou índice inválido".into());
            }
            if fragments.is_empty() {
                fragments.resize(count, None);
            }
            if fragments.len() != count {
                return Err("quantidade de fragmentos mudou durante a leitura da paleta".into());
            }
            let data = &bytes[5..5 + len];
            match &fragments[index] {
                Some(previous) if previous != data => {
                    return Err("fragmento duplicado de paleta contém dados diferentes".into());
                }
                Some(_) => (),
                None => fragments[index] = Some(data.to_vec()),
            }
            if fragments.iter().all(Option::is_some) {
                return Ok(fragments.into_iter().flatten().flatten().collect());
            }
        }
        if !retry {
            return Err("paleta incompleta: excesso de fragmentos duplicados".into());
        }
    }
    Err("tempo esgotado após cinco tentativas de ler a paleta".into())
}

fn write_data(t: &impl Transport, command: u8, data: &[u8]) -> Result<(), String> {
    let count = data.len().div_ceil(CHUNK);
    if count == 0 || count > 127 {
        return Err("tamanho de gravação inválido".into());
    }
    for (index, chunk) in data.chunks(CHUNK).enumerate() {
        // Both original SetLED/SetEData wrappers pass a 15 ms inter-packet delay.
        std::thread::sleep(Duration::from_millis(15));
        send(t, &packet(command, count as u8, index as u8, 0, chunk))?;
        let ack = receive(t, command, Instant::now() + Duration::from_secs(3))
            .map_err(|e| e.to_string())?;
        if usize::from(ack[3]) != index {
            return Err(format!(
                "confirmação do fragmento incorreto: {}, esperado {index}; gravação interrompida",
                ack[3]
            ));
        }
    }
    Ok(())
}

fn validate_marker(data: &[u8]) -> Result<(), String> {
    if !data.ends_with(&[0x5a, 0xa5]) {
        return Err("bloco de configuração sem marcador 5a a5; gravação não enviada".into());
    }
    Ok(())
}

fn require_known_model(hid: &Hidraw) -> Result<(), String> {
    let psd = query_psd(hid)?;
    if model(&psd).is_none() {
        return Err(format!(
            "PSD não reconhecido ({psd:02x?}); configuração não enviada"
        ));
    }
    Ok(())
}

/// Preserve all unknown LED bytes; callers should modify a successful read.
/// Confirm exact model identification again before sending configuration data.
pub fn set_led(hid: &Hidraw, data: &[u8; 128]) -> Result<(), String> {
    validate_marker(data)?;
    require_known_model(hid)?;
    if hid.wireless() {
        return write_data(hid, 4, data);
    }
    feature_write(hid, feature_id(hid)?, 4, 0, data)
}

/// The CRC=1 models use a separate 512-byte EData palette with a fixed marker.
/// Preserve all bytes outside the explicitly edited RGB triplets.
pub fn set_palette(hid: &Hidraw, data: &[u8]) -> Result<(), String> {
    if data.len() > 512 {
        return Err("paleta excede o limite recuperado de 512 bytes".into());
    }
    validate_marker(data)?;
    require_known_model(hid)?;
    if hid.wireless() {
        return write_data(hid, 9, data);
    }
    feature_write(hid, feature_id(hid)?, 0x0a, 0, data)
}

/// Select the wired feature520 channel or the dynamically discovered wireless
/// FF02 report19 channel; these are distinct backends for the same models.
pub fn open(device: &crate::device::Device) -> Result<Hidraw, String> {
    if !device.is_aula() {
        return Err("interface não pertence a um AULA suportado".into());
    }
    let descriptor = device
        .descriptor()
        .map_err(|e| format!("não foi possível ler descritor: {e}"))?;
    let reports = crate::descriptor::parse(&descriptor)?;
    let wired = reports.iter().any(|r| {
        r.kind == crate::descriptor::Kind::Feature
            && matches!(r.id, 6 | 9)
            && r.bytes() == 520
            && r.usage_pages.iter().any(|p| matches!(*p, 0xff00 | 0xff02))
    });
    let wireless = [
        crate::descriptor::Kind::Input,
        crate::descriptor::Kind::Output,
    ]
    .iter()
    .all(|kind| {
        reports.iter().any(|r| {
            r.kind == *kind && r.id == 19 && r.bytes() == 20 && r.usage_pages.contains(&0xff02)
        })
    });
    if !wired && !wireless {
        return Err("interface sem canal AULA reconhecido: esperado feature520 USB ou report19 FF02 de20bytes no receptor".into());
    }
    Hidraw::open(&device.path)
}

fn feature_id(hid: &Hidraw) -> Result<u8, String> {
    hid.wired_report_id().ok_or_else(|| {
        "canal feature AULA de 520 bytes ausente; use a interface do teclado por cabo USB".into()
    })
}

trait FeatureTransport {
    fn set(&self, report: &[u8]) -> io::Result<usize>;
    fn get(&self, report: &mut [u8]) -> io::Result<usize>;
}

impl FeatureTransport for Hidraw {
    fn set(&self, report: &[u8]) -> io::Result<usize> {
        self.send_feature(report)
    }
    fn get(&self, report: &mut [u8]) -> io::Result<usize> {
        self.get_feature(report)
    }
}

fn feature_packet(
    id: u8,
    command: u8,
    page: u8,
    count: usize,
    index: usize,
    len: usize,
) -> [u8; 520] {
    let mut bytes = [0; 520];
    bytes[..8].copy_from_slice(&[
        id,
        command,
        page,
        0,
        count as u8,
        index as u8,
        len as u8,
        (len >> 8) as u8,
    ]);
    bytes
}

fn feature_send(t: &impl FeatureTransport, bytes: &[u8; 520]) -> Result<(), String> {
    let n = t
        .set(bytes)
        .map_err(|e| format!("falha em SET_FEATURE AULA: {e}"))?;
    if n != bytes.len() {
        return Err(format!("SET_FEATURE incompleto: {n}/520"));
    }
    Ok(())
}

fn feature_read<const N: usize>(
    t: &impl FeatureTransport,
    id: u8,
    command: u8,
    page: u8,
) -> Result<[u8; N], String> {
    if N == 0 || N > 512 {
        return Err("leitura feature fora do limite recuperado".into());
    }
    // Read wrappers 499bf0/49a290, PSD49a530 and battery49a6c0 all use the
    // same header and 20 ms settling delays. No additive checksum in this family.
    let mut bytes = feature_packet(id, command, page, 1, 0, N);
    std::thread::sleep(Duration::from_millis(20));
    feature_send(t, &bytes)?;
    std::thread::sleep(Duration::from_millis(20));
    let n = t
        .get(&mut bytes)
        .map_err(|e| format!("falha em GET_FEATURE AULA: {e}"))?;
    if n < N + 8 || n > bytes.len() || bytes[0] != id {
        return Err(format!(
            "resposta GET_FEATURE inválida: {n} bytes, report ID {}",
            bytes[0]
        ));
    }
    let mut data = [0; N];
    data.copy_from_slice(&bytes[8..8 + N]);
    Ok(data)
}

fn feature_write(
    t: &impl FeatureTransport,
    id: u8,
    command: u8,
    page: u8,
    data: &[u8],
) -> Result<(), String> {
    if data.is_empty() || data.len() > 512 {
        return Err("gravação feature fora do limite recuperado".into());
    }
    let mut bytes = feature_packet(id, command, page, 1, 0, data.len());
    bytes[8..8 + data.len()].copy_from_slice(data);
    std::thread::sleep(Duration::from_millis(20));
    // Original write mode1 only sends SET_FEATURE. Callers verify with readback.
    feature_send(t, &bytes)
}

/// Raw power byte is consumed directly as the UI percentage in the driver;
/// out-of-range sentinel values remain unknown rather than being clamped.
pub fn battery(hid: &Hidraw) -> Result<Option<u8>, String> {
    if hid.wireless() {
        let power: [u8; 2] =
            read_data(hid, 0x4a).map_err(|e| format!("consulta de bateria wireless: {e}"))?;
        return Ok((power[0] <= 100).then_some(power[0]));
    }
    let power: [u8; 2] = feature_read(hid, feature_id(hid)?, 0x87, 2)?;
    Ok((power[0] <= 100).then_some(power[0]))
}

/// Recovered from 0x4950e0: mode byte 9=0; hardware effect ID at byte10.
#[cfg(test)]
pub fn set_effect(data: &mut [u8; 128], hardware: u8) {
    data[9] = 0;
    data[10] = hardware;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque};

    #[test]
    fn captured_f75_palette_read_has_490_bytes_and_separate_write_framing() {
        let psd = [3, 0, 0, 0, 3, 0xc2];
        // Seven RGB colors, repeated 23 times, followed by the captured PSD
        // and the opaque final byte. Reproduces all 35 report49 fragments.
        let colors = [
            255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0, 255, 0, 255, 0, 255, 255, 255, 255, 255,
        ];
        let mut raw = colors.repeat(23);
        raw.extend_from_slice(&psd);
        raw.push(1);
        assert_eq!(raw.len(), 490);
        let frames: Vec<_> = raw
            .chunks(14)
            .enumerate()
            .map(|(i, c)| packet(0x49, 35, i as u8, 0, c).to_vec())
            .collect();
        assert_eq!(
            frames[0],
            [
                0x13, 0x49, 0x23, 0, 0x0e, 0xff, 0, 0, 0, 0xff, 0, 0, 0, 0xff, 0xff, 0xff, 0, 0xff,
                0, 0x87
            ]
        );
        assert_eq!(
            frames[34],
            [
                0x13, 0x49, 0x23, 0x22, 0x0e, 0xff, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 3, 0, 0, 0, 3,
                0xc2, 1, 0x72
            ]
        );
        let received = read_variable(&Fake::new(frames), 0x49, 512).unwrap();
        let mut image = palette_write_image(&received, &psd).unwrap();
        assert_eq!(&image[..490], raw);
        assert_eq!(&image[490..510], [0; 20]);
        assert_eq!(&image[510..], [0x5a, 0xa5]);
        let before = image.clone();
        let mut led = [0; 128];
        led[10] = 1;
        led[126..].copy_from_slice(&[0x5a, 0xa5]);
        crate::rgb::apply(
            &mut led,
            Some(&mut image),
            "F75",
            crate::rgb::LedLayout::F75Wireless,
            None,
            Some([0x4c, 0xb5, 0xe8]),
            None,
        )
        .unwrap();
        assert_eq!(&image[..21], &before[..21]);
        assert_eq!(&image[42..], &before[42..]);
        for c in image[21..42].as_chunks::<3>().0 {
            assert_eq!(*c, [0x4c, 0xb5, 0xe8]);
        }
        // Readback exposes 490 bytes again; normalization must compare equal.
        assert_eq!(palette_write_image(&image[..490], &psd).unwrap(), image);
        assert!(palette_write_image(&raw[..489], &psd).is_err());
        assert!(palette_write_image(&raw, &[3, 0, 0, 0, 3, 0xc1]).is_err());
    }

    #[test]
    fn variable_palette_preserves_exact_35_fragment_payload() {
        // Synthetic 480-byte reply: the real report established count=35,
        // while the exact final length must come from its final fragment.
        for len in [480usize, 490, 512] {
            let expected: Vec<_> = (0..len).map(|i| (i % 251) as u8).collect();
            let count = len.div_ceil(CHUNK) as u8;
            let mut replies: Vec<_> = expected
                .chunks(CHUNK)
                .enumerate()
                .map(|(i, c)| packet(0x49, count, i as u8, 0, c).to_vec())
                .collect();
            replies.reverse();
            replies.insert(1, replies[0].clone());
            let fake = Fake::new(replies);
            assert_eq!(read_variable(&fake, 0x49, 512).unwrap(), expected);
        }
    }

    #[test]
    fn variable_palette_rejects_changed_count_and_oversized_payload() {
        let a = packet(0x49, 35, 0, 0, &[1; 14]).to_vec();
        let b = packet(0x49, 34, 1, 0, &[1; 14]).to_vec();
        assert!(
            read_variable(&Fake::new(vec![a, b]), 0x49, 512)
                .unwrap_err()
                .contains("mudou")
        );
        let oversized = packet(0x49, 37, 36, 0, &[1; 14]).to_vec();
        assert!(read_variable(&Fake::new(vec![oversized]), 0x49, 512).is_err());
    }
    struct Fake {
        replies: RefCell<VecDeque<Vec<u8>>>,
        sent: RefCell<Vec<Vec<u8>>>,
    }
    impl Fake {
        fn new(replies: Vec<Vec<u8>>) -> Self {
            Self {
                replies: RefCell::new(replies.into()),
                sent: RefCell::new(vec![]),
            }
        }
    }
    impl Transport for Fake {
        fn send(&self, p: &[u8]) -> io::Result<usize> {
            self.sent.borrow_mut().push(p.to_vec());
            Ok(p.len())
        }
        fn receive(&self, p: &mut [u8], _: Duration) -> io::Result<usize> {
            let r = self
                .replies
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "fixture exhausted"))?;
            p[..r.len()].copy_from_slice(&r);
            Ok(r.len())
        }
    }

    struct FeatureFake {
        response: Vec<u8>,
        sent: RefCell<Vec<Vec<u8>>>,
    }
    impl FeatureTransport for FeatureFake {
        fn set(&self, p: &[u8]) -> io::Result<usize> {
            self.sent.borrow_mut().push(p.to_vec());
            Ok(p.len())
        }
        fn get(&self, p: &mut [u8]) -> io::Result<usize> {
            p[..self.response.len()].copy_from_slice(&self.response);
            Ok(self.response.len())
        }
    }

    #[test]
    fn fw24_feature_psd_query_uses_report6_520_bytes_and_payload_offset8() {
        let mut response = vec![0; 520];
        response[0] = 6;
        response[8..14].copy_from_slice(&[3, 0, 0, 0, 3, 0xc2]);
        let fake = FeatureFake {
            response,
            sent: RefCell::new(vec![]),
        };
        assert_eq!(
            feature_read::<6>(&fake, 6, 0x82, 1).unwrap(),
            [3, 0, 0, 0, 3, 0xc2]
        );
        let sent = fake.sent.borrow();
        assert_eq!(sent[0].len(), 520);
        assert_eq!(&sent[0][..8], &[6, 0x82, 1, 0, 1, 0, 6, 0]);
        assert!(sent[0][8..].iter().all(|b| *b == 0));
    }

    #[test]
    fn fw24_led_palette_and_power_headers_match_separate_operations() {
        assert_eq!(
            &feature_packet(6, 0x84, 0, 1, 0, 128)[..8],
            &[6, 0x84, 0, 0, 1, 0, 128, 0]
        );
        assert_eq!(
            &feature_packet(6, 0x8a, 0, 1, 0, 512)[..8],
            &[6, 0x8a, 0, 0, 1, 0, 0, 2]
        );
        assert_eq!(
            &feature_packet(6, 0x87, 2, 1, 0, 2)[..8],
            &[6, 0x87, 2, 0, 1, 0, 2, 0]
        );
        let fake = FeatureFake {
            response: vec![],
            sent: RefCell::new(vec![]),
        };
        feature_write(&fake, 6, 0x0a, 0, &[0xab; 512]).unwrap();
        let sent = fake.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(&sent[0][..8], &[6, 0x0a, 0, 0, 1, 0, 0, 2]);
        assert!(sent[0][8..].iter().all(|b| *b == 0xab));
    }

    #[test]
    fn feature_read_rejects_short_or_wrong_report_replies() {
        for response in [vec![6; 8], vec![9; 520]] {
            let fake = FeatureFake {
                response,
                sent: RefCell::new(vec![]),
            };
            assert!(feature_read::<6>(&fake, 6, 0x82, 1).is_err());
        }
    }
    #[test]
    fn psd_request_matches_binary_and_decodes_all_known_models() {
        let request = packet(5, 1, 0, 0, &[]);
        assert_eq!(&request[..5], &[0x13, 5, 1, 0, 0]);
        assert_eq!(request[19], 0x19);
        for (id, name) in [(0xc1, "F87"), (0xc2, "F75"), (0xc3, "F99")] {
            let psd = [3, 0, 0, 0, 3, id];
            let fake = Fake::new(vec![packet(5, 1, 0, 0, &psd).to_vec()]);
            assert_eq!(read_data::<6>(&fake, 5).unwrap(), psd);
            assert_eq!(model(&psd), Some(name));
        }
        assert_eq!(model(&[0; 6]), None);
    }

    #[test]
    fn wireless_discovery_queries_state_before_psd() {
        let psd = [3, 0, 0, 0, 3, 0xc2];
        let fake = Fake::new(vec![
            packet(7, 1, 0, 0, &[1]).to_vec(),
            packet(5, 1, 0, 0, &psd).to_vec(),
        ]);
        assert_eq!(discover_wireless(&fake).unwrap(), psd);
        let sent = fake.sent.borrow();
        assert_eq!(sent.iter().map(|p| p[1]).collect::<Vec<_>>(), vec![7, 5]);
        assert_eq!(sent[0][19], 0x1b);
    }

    #[test]
    fn wireless_discovery_waits_for_connection_when_state_zero() {
        let psd = [3, 0, 0, 0, 3, 0xc1];
        let fake = Fake::new(vec![
            packet(7, 1, 0, 0, &[0]).to_vec(),
            packet(0x0a, 1, 0, 0, &[0, 1]).to_vec(),
            packet(5, 1, 0, 0, &psd).to_vec(),
        ]);
        assert_eq!(discover_wireless(&fake).unwrap(), psd);
        assert_eq!(fake.sent.borrow().len(), 2);
        let fake = Fake::new(vec![packet(7, 1, 0, 0, &[0]).to_vec()]);
        assert!(
            discover_wireless(&fake)
                .unwrap_err()
                .contains("respondeu desconectado")
        );
        assert_eq!(fake.sent.borrow().len(), 1);
    }

    #[test]
    fn wireless_discovery_retries_state_timeout_then_attempts_psd() {
        struct InitiallyTimeout {
            inner: Fake,
            left: std::cell::Cell<usize>,
        }
        impl Transport for InitiallyTimeout {
            fn send(&self, p: &[u8]) -> io::Result<usize> {
                self.inner.send(p)
            }
            fn receive(&self, p: &mut [u8], timeout: Duration) -> io::Result<usize> {
                if self.left.get() > 0 {
                    self.left.set(self.left.get() - 1);
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "fixture timeout"));
                }
                self.inner.receive(p, timeout)
            }
        }
        let psd = [3, 0, 0, 0, 3, 0xc3];
        let fake = InitiallyTimeout {
            inner: Fake::new(vec![packet(5, 1, 0, 0, &psd).to_vec()]),
            left: std::cell::Cell::new(5),
        };
        assert_eq!(discover_wireless(&fake).unwrap(), psd);
        assert_eq!(
            fake.inner
                .sent
                .borrow()
                .iter()
                .map(|p| p[1])
                .collect::<Vec<_>>(),
            vec![7, 7, 7, 7, 7, 5]
        );
    }
    #[test]
    fn led_read_reassembles_reordered_and_duplicate_fragments() {
        let expected = std::array::from_fn::<_, 128, _>(|i| i as u8);
        let mut fragments: Vec<_> = expected
            .chunks(14)
            .enumerate()
            .map(|(i, c)| packet(0x44, 10, i as u8, 0, c).to_vec())
            .collect();
        fragments.reverse();
        fragments.insert(1, fragments[0].clone());
        let fake = Fake::new(fragments);
        assert_eq!(read_data::<128>(&fake, 0x44).unwrap(), expected);
        assert_eq!(fake.sent.borrow()[0][19], 0x58);
    }

    #[test]
    fn query_accepts_final_packet_padding_but_not_missing_data() {
        let mut payload = [0; 14];
        payload[..6].copy_from_slice(&[3, 0, 0, 0, 3, 0xc2]);
        let fake = Fake::new(vec![packet(5, 1, 0, 0, &payload).to_vec()]);
        assert_eq!(read_data::<6>(&fake, 5).unwrap(), [3, 0, 0, 0, 3, 0xc2]);
        let fake = Fake::new(vec![packet(5, 1, 0, 0, &payload[..5]).to_vec()]);
        assert!(read_data::<6>(&fake, 5).is_err());
    }
    #[test]
    fn read_rejects_malformed_lengths_indices_truncation_and_status() {
        let good = packet(5, 1, 0, 0, &[3, 0, 0, 0, 3, 0xc2]);
        let mut bad_len = good;
        bad_len[4] = 15;
        let mut bad_index = good;
        bad_index[3] = 1;
        let mut rejected = good;
        rejected[2] |= 0x80;
        for bad in [
            bad_len.to_vec(),
            bad_index.to_vec(),
            good[..10].to_vec(),
            rejected.to_vec(),
        ] {
            assert!(read_data::<6>(&Fake::new(vec![bad]), 5).is_err());
        }
    }
    #[test]
    fn write_fragments_128_bytes_and_stops_on_wrong_ack() {
        let fake = Fake::new((0..10).map(|i| packet(4, 10, i, 0, &[]).to_vec()).collect());
        write_data(&fake, 4, &[0xab; 128]).unwrap();
        let sent = fake.sent.borrow();
        assert_eq!(sent.len(), 10);
        assert_eq!(&sent[0][..5], &[0x13, 4, 10, 0, 14]);
        assert_eq!(&sent[9][..7], &[0x13, 4, 10, 9, 2, 0xab, 0xab]);
        assert!(sent[9][7..19].iter().all(|b| *b == 0));
        drop(sent);
        let fake = Fake::new(vec![packet(4, 10, 1, 0, &[]).to_vec()]);
        assert!(write_data(&fake, 4, &[0; 128]).is_err());
        assert_eq!(fake.sent.borrow().len(), 1);
    }

    #[test]
    fn palette_uses_37_fragments_and_correct_commands() {
        let expected = std::array::from_fn::<_, 512, _>(|i| (i % 251) as u8);
        let fragments = expected
            .chunks(14)
            .enumerate()
            .map(|(i, c)| packet(0x49, 37, i as u8, 0, c).to_vec())
            .collect();
        let fake = Fake::new(fragments);
        assert_eq!(read_data::<512>(&fake, 0x49).unwrap(), expected);
        assert_eq!(&fake.sent.borrow()[0][..5], &[0x13, 0x49, 1, 0, 0]);
        assert_eq!(fake.sent.borrow()[0][19], 0x5d);
        let fake = Fake::new((0..37).map(|i| packet(9, 37, i, 0, &[]).to_vec()).collect());
        write_data(&fake, 9, &expected).unwrap();
        let sent = fake.sent.borrow();
        assert_eq!(sent.len(), 37);
        assert_eq!(&sent[36][..5], &[0x13, 9, 37, 36, 8]);
        assert_eq!(&sent[36][5..13], &expected[504..512]);
        assert!(sent[36][13..19].iter().all(|b| *b == 0));
    }

    #[test]
    fn configuration_marker_must_be_preserved_at_exact_block_end() {
        for len in [128, 512] {
            let mut data = vec![0; len];
            assert!(validate_marker(&data).is_err());
            data[len - 2..].copy_from_slice(&[0x5a, 0xa5]);
            assert!(validate_marker(&data).is_ok());
            data[len - 1] = 0;
            assert!(validate_marker(&data).is_err());
        }
    }
    #[test]
    fn effect_edit_preserves_unrelated_configuration() {
        let mut data = [0xa5; 128];
        set_effect(&mut data, 19);
        assert_eq!(data[9], 0);
        assert_eq!(data[10], 19);
        assert!(
            data.iter()
                .enumerate()
                .all(|(i, b)| i == 9 || i == 10 || *b == 0xa5)
        );
    }
}
