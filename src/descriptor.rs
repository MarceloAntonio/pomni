use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Input,
    Output,
    Feature,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub kind: Kind,
    pub id: u8,
    pub bits: u64,
    pub usage_pages: Vec<u32>,
}

impl Report {
    pub fn bytes(&self) -> usize {
        self.bits.div_ceil(8) as usize + usize::from(self.id != 0)
    }
}

#[derive(Clone, Copy, Default)]
struct Global {
    page: u32,
    size: u32,
    count: u32,
    id: u8,
}

/// Decode short HID items; long items are skipped according to their declared length.
/// Report lengths include all main fields, including padding, and the report ID when present.
pub fn parse(bytes: &[u8]) -> Result<Vec<Report>, String> {
    let mut pos = 0;
    let mut state = Global::default();
    let mut stack = Vec::new();
    let mut reports = BTreeMap::<(Kind, u8), Report>::new();
    while pos < bytes.len() {
        let prefix = bytes[pos];
        pos += 1;
        if prefix == 0xfe {
            if bytes.len() - pos < 2 {
                return Err("item HID longo truncado".into());
            }
            let len = bytes[pos] as usize;
            pos += 2;
            if bytes.len() - pos < len {
                return Err("item HID longo truncado".into());
            }
            pos += len;
            continue;
        }
        let len = match prefix & 3 {
            3 => 4,
            n => n as usize,
        };
        if bytes.len() - pos < len {
            return Err("item HID truncado".into());
        }
        let mut value = 0u32;
        for (i, b) in bytes[pos..pos + len].iter().enumerate() {
            value |= (*b as u32) << (8 * i);
        }
        pos += len;
        match ((prefix >> 2) & 3, prefix >> 4) {
            (1, 0) => state.page = value,
            (1, 7) => state.size = value,
            (1, 8) => {
                if !(1..=255).contains(&value) {
                    return Err("Report ID HID inválido".into());
                }
                state.id = value as u8;
            }
            (1, 9) => state.count = value,
            (1, 10) => stack.push(state),
            (1, 11) => state = stack.pop().ok_or("POP HID sem PUSH")?,
            (0, tag @ (8 | 9 | 11)) => {
                let kind = match tag {
                    8 => Kind::Input,
                    9 => Kind::Output,
                    _ => Kind::Feature,
                };
                let r = reports.entry((kind, state.id)).or_insert(Report {
                    kind,
                    id: state.id,
                    bits: 0,
                    usage_pages: Vec::new(),
                });
                r.bits = r
                    .bits
                    .checked_add(u64::from(state.size) * u64::from(state.count))
                    .ok_or("comprimento HID excede o limite")?;
                if r.bits > 1_048_576 {
                    return Err("comprimento HID excessivo".into());
                }
                if !r.usage_pages.contains(&state.page) {
                    r.usage_pages.push(state.page);
                }
            }
            _ => {}
        }
    }
    Ok(reports.into_values().collect())
}

pub fn summary(bytes: &[u8]) -> String {
    match parse(bytes) {
        Ok(reports) if reports.is_empty() => "nenhum relatório HID declarado".into(),
        Ok(reports) => reports
            .iter()
            .map(|r| {
                let pages = r
                    .usage_pages
                    .iter()
                    .map(|p| format!("0x{p:04x}"))
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{:?} id={} bytes={} pages={pages}", r.kind, r.id, r.bytes())
            })
            .collect::<Vec<_>>()
            .join("; "),
        Err(e) => format!("descritor não decodificado: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn computes_wireless_feature_size_including_id() {
        let reports = parse(&[
            0x06, 0x04, 0xff, 0x85, 0x06, 0x75, 0x08, 0x95, 0x07, 0xb1, 0x02,
        ])
        .unwrap();
        assert_eq!(
            reports[0],
            Report {
                kind: Kind::Feature,
                id: 6,
                bits: 56,
                usage_pages: vec![0xff04]
            }
        );
        assert_eq!(reports[0].bytes(), 8);
    }
    #[test]
    fn accumulates_padding_and_restores_global_stack() {
        let r = parse(&[
            0x75, 1, 0x95, 3, 0x81, 2, 0xa4, 0x95, 5, 0x81, 1, 0xb4, 0x81, 2,
        ])
        .unwrap();
        assert_eq!(r[0].bits, 11);
        assert_eq!(r[0].bytes(), 2);
    }
    #[test]
    fn malformed_items_fail() {
        assert!(parse(&[0x76, 1]).is_err());
        assert!(parse(&[0xb4]).is_err());
        assert!(parse(&[0x85, 0]).is_err());
        assert!(parse(&[0xfe, 6, 1, 2]).is_err());
    }
}
