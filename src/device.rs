use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Device {
    pub path: PathBuf,
    pub sysfs: PathBuf,
    pub bus: u16,
    pub vid: u16,
    pub pid: u16,
    pub name: String,
    pub phys: String,
    pub uniq: String,
}

impl Device {
    pub fn is_aula(&self) -> bool {
        match self.bus {
            0x0003 => matches!((self.vid, self.pid), (0x258a, 0x010c) | (0x3554, 0xfa09)),
            0x0005 => {
                matches!(self.vid, 0x258a | 0x3554)
                    && ["AULA F75", "AULA F87", "AULA F99"]
                        .iter()
                        .any(|n| self.name.trim().eq_ignore_ascii_case(n))
            }
            _ => false,
        }
    }

    pub fn mode(&self) -> &'static str {
        if !self.is_aula() {
            return "desconhecido";
        }
        match self.bus {
            0x0005 => "bluetooth",
            0x0003 if self.vid == 0x3554 => "wireless (receptor detectado)",
            0x0003 => "cabo",
            _ => "desconhecido",
        }
    }

    pub fn supports_report19(&self) -> bool {
        let Ok(bytes) = self.descriptor() else {
            return false;
        };
        let Ok(reports) = crate::descriptor::parse(&bytes) else {
            return false;
        };
        [
            crate::descriptor::Kind::Input,
            crate::descriptor::Kind::Output,
        ]
        .iter()
        .all(|kind| {
            reports.iter().any(|r| {
                r.kind == *kind && r.id == 19 && r.bytes() == 20 && r.usage_pages.contains(&0xff02)
            })
        })
    }

    pub fn supports_configuration(&self) -> bool {
        self.supports_wired_configuration()
            || (self.is_aula() && self.bus == 3 && self.supports_report19())
    }

    fn supports_wired_configuration(&self) -> bool {
        if !self.is_aula() || self.bus != 0x0003 {
            return false;
        }
        let Ok(bytes) = self.descriptor() else {
            return false;
        };
        let Ok(reports) = crate::descriptor::parse(&bytes) else {
            return false;
        };
        reports.iter().any(|r| {
            r.kind == crate::descriptor::Kind::Feature
                && r.bytes() == 520
                && ((r.id == 6 && r.usage_pages.contains(&0xff00))
                    || (r.id == 9 && r.usage_pages.contains(&0xff02)))
        })
    }

    /// The nearest matching USB device owns its HID interfaces. Never use an
    /// upstream hub or the machine-wide power_supply class as the search root.
    fn physical_root(&self) -> Result<PathBuf, String> {
        let hid = fs::canonicalize(&self.sysfs)
            .map_err(|e| format!("não foi possível resolver {}: {e}", self.sysfs.display()))?;
        if self.bus == 0x0003 {
            for ancestor in hid.ancestors() {
                let id = |name: &str| {
                    fs::read_to_string(ancestor.join(name))
                        .ok()
                        .and_then(|s| u16::from_str_radix(s.trim(), 16).ok())
                };
                if id("idVendor") == Some(self.vid) && id("idProduct") == Some(self.pid) {
                    return Ok(ancestor.to_path_buf());
                }
            }
        }
        Ok(hid)
    }

    fn group_key(&self) -> String {
        if self.bus == 0x0005 && !self.uniq.trim().is_empty() {
            return format!(
                "bt:{:04x}:{:04x}:{}",
                self.vid,
                self.pid,
                self.uniq.trim().to_ascii_lowercase()
            );
        }
        match self.physical_root() {
            Ok(root) => format!("sysfs:{}", root.display()),
            Err(_) => format!("interface:{}", self.path.display()),
        }
    }

    /// Return only a capacity exported by a power supply under this exact
    /// physical device. Absence means unknown, never an invented percentage.
    pub fn battery(&self) -> Result<Option<u8>, String> {
        let root = self.physical_root()?;
        let mut visited = HashSet::new();
        let mut values = BTreeSet::new();
        scan_battery(&root, &root, 0, &mut visited, &mut values)?;
        match values.len() {
            0 => Ok(None),
            1 => Ok(values.first().copied()),
            _ => Err(
                "mais de uma capacidade de bateria diferente foi encontrada para este dispositivo"
                    .into(),
            ),
        }
    }
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let s = path.to_str().ok_or("caminho de dispositivo inválido")?;
        crate::cli::device_path(s)?;
        let name = path.file_name().ok_or("caminho de dispositivo inválido")?;
        let sysfs = Path::new("/sys/class/hidraw").join(name).join("device");
        let uevent = fs::read_to_string(sysfs.join("uevent"))
            .map_err(|e| format!("não foi possível consultar {}: {e}", path.display()))?;
        Self::parse(path.to_path_buf(), sysfs, &uevent)
    }
    fn parse(path: PathBuf, sysfs: PathBuf, uevent: &str) -> Result<Self, String> {
        let field = |key: &str| {
            uevent
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .unwrap_or("")
                .to_string()
        };
        let id = field("HID_ID=");
        let ids: Vec<_> = id.split(':').collect();
        if ids.len() != 3 {
            return Err(format!("HID_ID inválido para {}", path.display()));
        }
        let parse = |s| u32::from_str_radix(s, 16).map_err(|_| "HID_ID inválido".to_string());
        let bus = parse(ids[0])?;
        let vid = parse(ids[1])?;
        let pid = parse(ids[2])?;
        if bus > u16::MAX as u32 || vid > u16::MAX as u32 || pid > u16::MAX as u32 {
            return Err("HID_ID fora do intervalo".into());
        }
        Ok(Self {
            path,
            sysfs,
            bus: bus as u16,
            vid: vid as u16,
            pid: pid as u16,
            name: field("HID_NAME="),
            phys: field("HID_PHYS="),
            uniq: field("HID_UNIQ="),
        })
    }
    pub fn descriptor(&self) -> io::Result<Vec<u8>> {
        fs::read(self.sysfs.join("report_descriptor"))
    }
}

fn scan_battery(
    root: &Path,
    directory: &Path,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
    values: &mut BTreeSet<u8>,
) -> Result<(), String> {
    if depth > 6 {
        return Ok(());
    }
    let resolved = fs::canonicalize(directory)
        .map_err(|e| format!("não foi possível consultar {}: {e}", directory.display()))?;
    if !resolved.starts_with(root) || !visited.insert(resolved.clone()) {
        return Ok(());
    }
    let entries = fs::read_dir(&resolved)
        .map_err(|e| format!("não foi possível consultar {}: {e}", resolved.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Ok(child) = fs::canonicalize(&path) else {
            continue;
        };
        if !child.starts_with(root) {
            continue;
        }
        if entry.file_name() == "power_supply" {
            for supply in fs::read_dir(&child).map_err(|e| e.to_string())? {
                let supply = supply.map_err(|e| e.to_string())?;
                let capacity = supply.path().join("capacity");
                let canonical = match fs::canonicalize(&capacity) {
                    Ok(p) => p,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                    Err(e) => {
                        return Err(format!(
                            "não foi possível consultar {}: {e}",
                            capacity.display()
                        ));
                    }
                };
                if !canonical.starts_with(root) {
                    continue;
                }
                let text = fs::read_to_string(&canonical)
                    .map_err(|e| format!("não foi possível ler {}: {e}", canonical.display()))?;
                let value = text
                    .trim()
                    .parse::<u8>()
                    .ok()
                    .filter(|v| *v <= 100)
                    .ok_or_else(|| {
                        format!("capacidade de bateria inválida em {}", canonical.display())
                    })?;
                values.insert(value);
            }
        } else {
            scan_battery(root, &child, depth + 1, visited, values)?;
        }
    }
    Ok(())
}

pub fn detect() -> Result<Device, String> {
    detect_from(enumerate().map_err(|e| format!("não foi possível listar interfaces HID: {e}"))?)
}

fn detect_from(devices: Vec<Device>) -> Result<Device, String> {
    let mut groups = BTreeMap::<String, Vec<Device>>::new();
    for d in devices.into_iter().filter(Device::is_aula) {
        groups.entry(d.group_key()).or_default().push(d);
    }
    if groups.is_empty() {
        return Err("teclado desconhecido: nenhum AULA F75/F87/F99 reconhecido foi encontrado em cabo, wireless ou Bluetooth".into());
    }
    // A plugged receiver is not evidence that its keyboard is active. Prefer
    // an actual configuration endpoint, then a directly attached keyboard.
    if groups
        .values()
        .any(|g| g.iter().any(Device::supports_wired_configuration))
    {
        groups.retain(|_, g| g.iter().any(Device::supports_wired_configuration));
    } else if groups
        .values()
        .any(|g| g[0].bus != 0x0003 || g[0].vid != 0x3554)
    {
        groups.retain(|_, g| g[0].bus != 0x0003 || g[0].vid != 0x3554);
    }
    if groups.len() > 1 {
        let choices = groups
            .values()
            .map(|g| format!("{} ({})", g[0].path.display(), g[0].mode()))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "mais de um teclado AULA encontrado: {choices}. Desconecte os teclados adicionais para selecionar um automaticamente"
        ));
    }
    let mut interfaces = groups.into_values().next().expect("one group was checked");
    interfaces.sort_by_key(|d| {
        (
            std::cmp::Reverse(d.supports_wired_configuration()),
            std::cmp::Reverse(d.supports_report19()),
            d.path.clone(),
        )
    });
    Ok(interfaces.remove(0))
}

pub fn enumerate() -> io::Result<Vec<Device>> {
    let entries = match fs::read_dir("/sys/class/hidraw") {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut devices = Vec::new();
    for e in entries {
        let e = e?;
        let path = Path::new("/dev").join(e.file_name());
        match Device::from_path(&path) {
            Ok(d) => devices.push(d),
            Err(e) => eprintln!("Aviso: {e}"),
        }
    }
    devices.sort_by_key(|d| {
        d.path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("hidraw"))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    });
    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const REPORT19: &[u8] = &[
        0x06, 0x02, 0xff, 0x85, 19, 0x75, 8, 0x95, 19, 0x81, 0, 0x91, 0,
    ];
    const CONFIG: &[u8] = &[
        0x06, 0x00, 0xff, 0x85, 6, 0x75, 8, 0x96, 0x07, 0x02, 0xb1, 2,
    ];
    static SERIAL: AtomicUsize = AtomicUsize::new(0);

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "aula-device-test-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn usb(&self, unit: &str, interface: &str, wired: bool, descriptor: &[u8]) -> Device {
            let root = self.0.join(unit);
            let hid = root.join(interface).join("hid");
            fs::create_dir_all(&hid).unwrap();
            let (vid, pid) = if wired {
                ("258a", "010c")
            } else {
                ("3554", "fa09")
            };
            fs::write(root.join("idVendor"), vid).unwrap();
            fs::write(root.join("idProduct"), pid).unwrap();
            fs::write(hid.join("report_descriptor"), descriptor).unwrap();
            Device::parse(
                format!("/dev/hidraw{interface}").into(),
                hid,
                &format!("HID_ID=0003:0000{vid}:0000{pid}\nHID_NAME=Compx\n"),
            )
            .unwrap()
        }
        fn bt(&self, interface: &str, vendor: &str, name: &str, uniq: &str) -> Device {
            let hid = self.0.join(interface);
            fs::create_dir(&hid).unwrap();
            fs::write(hid.join("report_descriptor"), []).unwrap();
            Device::parse(
                format!("/dev/hidraw{interface}").into(),
                hid,
                &format!("HID_ID=0005:0000{vendor}:00001234\nHID_NAME={name}\nHID_UNIQ={uniq}\n"),
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn composite_interfaces_are_one_keyboard_and_configuration_wins() {
        let f = Fixture::new();
        let keyboard = f.usb("unit", "0", true, &[]);
        let vendor = f.usb("unit", "1", true, REPORT19);
        let config = f.usb("unit", "2", true, CONFIG);
        let selected = detect_from(vec![keyboard, vendor, config]).unwrap();
        assert_eq!(selected.path, Path::new("/dev/hidraw2"));
        assert!(selected.supports_configuration());
        assert!(!selected.supports_report19());
        assert_eq!(selected.mode(), "cabo");
    }

    #[test]
    fn idle_receiver_does_not_override_connected_cable() {
        let f = Fixture::new();
        let receiver = f.usb("receiver", "0", false, REPORT19);
        let cable = f.usb("keyboard", "1", true, CONFIG);
        assert_eq!(detect_from(vec![receiver, cable]).unwrap().mode(), "cabo");
    }

    #[test]
    fn configuration_requires_full_size_and_correct_report_variant() {
        let f = Fixture::new();
        let small_feature = [0x06, 0x04, 0xff, 0x85, 6, 0x75, 8, 0x95, 7, 0xb1, 2];
        assert!(
            !f.usb("receiver", "0", false, &small_feature)
                .supports_configuration()
        );
        let variant26 = [
            0x06, 0x02, 0xff, 0x85, 9, 0x75, 8, 0x96, 0x07, 0x02, 0xb1, 2,
        ];
        assert!(
            f.usb("wired", "1", true, &variant26)
                .supports_configuration()
        );
    }

    #[test]
    fn multiple_physical_keyboards_require_disambiguation() {
        let f = Fixture::new();
        let one = f.usb("one", "0", true, CONFIG);
        let two = f.usb("two", "1", true, CONFIG);
        let e = detect_from(vec![one, two]).unwrap_err();
        assert!(e.contains("mais de um teclado"));
        assert!(!e.contains("--device"));
    }

    #[test]
    fn bluetooth_requires_exact_model_name_and_known_vendor() {
        let f = Fixture::new();
        let known = f.bt("0", "3554", "AULA F75", "aa:bb");
        assert!(known.is_aula());
        assert_eq!(known.mode(), "bluetooth");
        assert!(!known.supports_configuration());
        assert!(
            !f.bt("1", "3554", "AULA F75 fake keyboard", "cc:dd")
                .is_aula()
        );
        assert!(!f.bt("2", "1234", "AULA F75", "ee:ff").is_aula());
        assert!(
            detect_from(vec![f.bt("3", "1234", "AULA F75", "11:22")])
                .unwrap_err()
                .contains("teclado desconhecido")
        );
    }

    #[test]
    fn bluetooth_interfaces_group_by_exact_device_address() {
        let f = Fixture::new();
        let one = f.bt("0", "3554", "AULA F75", "aa:bb");
        let sibling = f.bt("1", "3554", "AULA F75", "AA:BB");
        assert!(detect_from(vec![one.clone(), sibling]).is_ok());
        let other = f.bt("2", "3554", "AULA F75", "cc:dd");
        assert!(detect_from(vec![one, other]).is_err());
    }

    #[test]
    fn receiver_only_is_recognized_without_claiming_active_keyboard() {
        let f = Fixture::new();
        let selected = detect_from(vec![f.usb("receiver", "0", false, REPORT19)]).unwrap();
        assert_eq!(selected.mode(), "wireless (receptor detectado)");
        assert!(selected.supports_report19());
        assert!(selected.supports_configuration());
        assert_eq!(selected.battery().unwrap(), None);
    }

    #[test]
    fn battery_stays_inside_physical_device_and_validates_capacity() {
        let f = Fixture::new();
        let d = f.usb("keyboard", "0", true, CONFIG);
        let other = f.usb("other", "1", true, CONFIG);
        let unrelated = other.sysfs.join("power_supply/battery");
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(unrelated.join("capacity"), "99\n").unwrap();
        // A sysfs-style symlink to another device must not be followed.
        std::os::unix::fs::symlink(
            other.sysfs.join("power_supply"),
            d.sysfs.join("power_supply"),
        )
        .unwrap();
        assert_eq!(d.battery().unwrap(), None);
        fs::remove_file(d.sysfs.join("power_supply")).unwrap();
        let battery = d.sysfs.join("power_supply/battery");
        fs::create_dir_all(&battery).unwrap();
        fs::write(battery.join("capacity"), "42\n").unwrap();
        assert_eq!(d.battery().unwrap(), Some(42));
        fs::write(battery.join("capacity"), "101").unwrap();
        assert!(d.battery().is_err());
        fs::write(battery.join("capacity"), "bad").unwrap();
        assert!(d.battery().is_err());
    }

    #[test]
    fn ids_are_parsed_as_hex_and_filtered_exactly() {
        let mk = |id: &str| {
            Device::parse(
                "/dev/hidraw5".into(),
                "/sys/test".into(),
                &format!("HID_ID={id}\nHID_NAME=AULA\n"),
            )
        };
        assert!(mk("0003:00003554:0000FA09").unwrap().is_aula());
        assert!(!mk("0003:00003554:0000FA08").unwrap().is_aula());
        assert!(mk("0003:100003554:0000FA09").is_err());
    }
}
