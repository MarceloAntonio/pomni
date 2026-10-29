//! Linux hidraw transport. Buffers include the report ID in byte zero.
//! Opening a handle sends no reports; input reads are nonblocking and poll-bounded.
use std::{
    fs::{File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::Path,
};

use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};

#[repr(C)]
struct PollFd {
    fd: std::ffi::c_int,
    events: std::ffi::c_short,
    revents: std::ffi::c_short,
}

unsafe extern "C" {
    fn ioctl(fd: std::ffi::c_int, request: std::ffi::c_ulong, ...) -> std::ffi::c_int;
    fn poll(
        fds: *mut PollFd,
        count: std::ffi::c_ulong,
        timeout_ms: std::ffi::c_int,
    ) -> std::ffi::c_int;
}

pub struct Hidraw {
    file: File,
    wired_report_id: Option<u8>,
    wireless: bool,
}

impl Hidraw {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new().read(true).write(true).custom_flags(0x800 /* O_NONBLOCK */).open(path).map_err(|e| {
            if e.kind() == io::ErrorKind::PermissionDenied {
                format!("sem permissão para abrir {}: {e}. Configure acesso hidraw para seu usuário (consulte README.md)", path.display())
            } else { format!("não foi possível abrir {}: {e}", path.display()) }
        })?;
        let name = path.file_name().ok_or("caminho hidraw inválido")?;
        let descriptor = std::fs::read(
            Path::new("/sys/class/hidraw")
                .join(name)
                .join("device/report_descriptor"),
        )
        .map_err(|e| format!("não foi possível ler descritor HID: {e}"))?;
        let wired_report_id = select_wired_report(&descriptor)?;
        let wireless = select_wireless(&descriptor)?;
        Ok(Self {
            file,
            wired_report_id,
            wireless,
        })
    }

    pub fn wired_report_id(&self) -> Option<u8> {
        self.wired_report_id
    }

    pub fn wireless(&self) -> bool {
        self.wireless && self.wired_report_id.is_none()
    }

    pub fn send_feature(&self, report: &[u8]) -> io::Result<usize> {
        let request = feature_request(6, report.len())?;
        // SAFETY: the synchronous ioctl receives a valid readable report slice.
        let n = unsafe { ioctl(self.file.as_raw_fd(), request, report.as_ptr()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    pub fn get_feature(&self, report: &mut [u8]) -> io::Result<usize> {
        let request = feature_request(7, report.len())?;
        // SAFETY: the synchronous ioctl receives a valid writable report slice.
        let n = unsafe { ioctl(self.file.as_raw_fd(), request, report.as_mut_ptr()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    /// Write one output report in one write syscall. A partial write is an error:
    /// splitting the remaining bytes would send a second malformed report.
    /// WouldBlock is returned to the caller, which can retry the entire report.
    pub fn write_output(&self, report: &[u8]) -> io::Result<usize> {
        if report.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "output report vazio",
            ));
        }
        loop {
            match (&self.file).write(report) {
                Ok(n) if n == report.len() => return Ok(n),
                Ok(n) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        format!(
                            "output report incompleto: {n}/{} bytes; não reenviado",
                            report.len()
                        ),
                    ));
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Read one queued input report. Zero timeout checks the queue immediately.
    /// Timeout returns TimedOut; disconnected handles return an error, not zero bytes.
    pub fn read_input(&self, report: &mut [u8], timeout: Duration) -> io::Result<usize> {
        if report.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "buffer de input vazio",
            ));
        }
        let started = Instant::now();
        let mut first = true;
        loop {
            if !first && started.elapsed() >= timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "tempo esgotado aguardando input report",
                ));
            }
            first = false;
            let remaining = timeout.saturating_sub(started.elapsed());
            // Round upward so a positive sub-millisecond remainder never becomes
            // a busy poll; cap individual waits without truncating long timeouts.
            let milliseconds = remaining
                .as_nanos()
                .div_ceil(1_000_000)
                .min(i32::MAX as u128) as i32;
            let mut fd = PollFd {
                fd: self.file.as_raw_fd(),
                events: 1, /* POLLIN */
                revents: 0,
            };
            // SAFETY: fd is one initialized pollfd, alive and writable for this call.
            let result = unsafe { poll(&mut fd, 1, milliseconds) };
            if result < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
            if result == 0 {
                continue;
            }
            if fd.revents & 0x20 != 0
            /* POLLNVAL */
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "handle hidraw inválido",
                ));
            }
            if fd.revents & 1 != 0 {
                match (&self.file).read(report) {
                    Ok(0) => {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "dispositivo desconectado",
                        ));
                    }
                    Ok(n) => return Ok(n),
                    Err(e)
                        if matches!(
                            e.kind(),
                            io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                        ) =>
                    {
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            if fd.revents & 0x18 != 0
            /* POLLERR | POLLHUP */
            {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "dispositivo desconectado ou com erro de I/O",
                ));
            }
        }
    }
}

fn feature_request(number: u8, len: usize) -> io::Result<std::ffi::c_ulong> {
    if len == 0 || len >= (1 << 14) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "comprimento feature inválido",
        ));
    }
    Ok(
        ((3u32 << 30) | ((len as u32) << 16) | (u32::from(b'H') << 8) | u32::from(number))
            as std::ffi::c_ulong,
    )
}

fn select_wired_report(bytes: &[u8]) -> Result<Option<u8>, String> {
    let reports = crate::descriptor::parse(bytes)?;
    Ok(reports
        .iter()
        .find(|r| {
            r.kind == crate::descriptor::Kind::Feature
                && matches!(r.id, 6 | 9)
                && r.bytes() == 520
                && r.usage_pages.iter().any(|p| matches!(*p, 0xff00 | 0xff02))
        })
        .map(|r| r.id))
}

fn select_wireless(bytes: &[u8]) -> Result<bool, String> {
    let reports = crate::descriptor::parse(bytes)?;
    Ok([
        crate::descriptor::Kind::Input,
        crate::descriptor::Kind::Output,
    ]
    .iter()
    .all(|kind| {
        reports.iter().any(|r| {
            r.kind == *kind && r.id == 19 && r.bytes() == 20 && r.usage_pages.contains(&0xff02)
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_large_vendor_feature_reports_select_configuration_protocol() {
        let descriptor = |id: u8, count: u16| {
            vec![
                0x06,
                0,
                0xff,
                0x09,
                1,
                0xa1,
                1,
                0x85,
                id,
                0x75,
                8,
                0x96,
                count as u8,
                (count >> 8) as u8,
                0xb1,
                2,
                0xc0,
            ]
        };
        assert_eq!(select_wired_report(&descriptor(6, 519)).unwrap(), Some(6));
        assert_eq!(select_wired_report(&descriptor(9, 519)).unwrap(), Some(9));
        assert_eq!(select_wired_report(&descriptor(6, 7)).unwrap(), None);
        assert_eq!(feature_request(6, 520).unwrap(), 0xc2084806);
        assert_eq!(feature_request(7, 520).unwrap(), 0xc2084807);
        let wireless = [
            0x06, 2, 0xff, 0x09, 2, 0xa1, 1, 0x85, 19, 0x75, 8, 0x95, 19, 0x81, 2, 0x91, 2, 0xc0,
        ];
        assert!(select_wireless(&wireless).unwrap());
        assert_eq!(select_wired_report(&wireless).unwrap(), None);
        assert!(!select_wireless(&descriptor(6, 7)).unwrap());
    }

    fn socket_transport() -> (Hidraw, std::os::unix::net::UnixStream) {
        use std::os::fd::OwnedFd;
        let (a, b) = std::os::unix::net::UnixStream::pair().unwrap();
        a.set_nonblocking(true).unwrap();
        let owned: OwnedFd = a.into();
        (
            Hidraw {
                file: File::from(owned),
                wired_report_id: None,
                wireless: true,
            },
            b,
        )
    }

    #[test]
    fn input_timeout_does_not_block_and_queued_report_is_read() {
        let (transport, mut peer) = socket_transport();
        let mut buffer = [0; 20];
        assert_eq!(
            transport
                .read_input(&mut buffer, Duration::ZERO)
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        peer.write_all(&[19, 1, 2]).unwrap();
        let n = transport.read_input(&mut buffer, Duration::ZERO).unwrap();
        assert_eq!(&buffer[..n], &[19, 1, 2]);
    }

    #[test]
    fn input_reports_disconnect_instead_of_waiting() {
        let (transport, peer) = socket_transport();
        drop(peer);
        let e = transport
            .read_input(&mut [0; 20], Duration::from_secs(1))
            .unwrap_err();
        assert!(matches!(
            e.kind(),
            io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe
        ));
    }

    #[test]
    fn positive_input_timeout_expires_without_shortening_the_deadline() {
        let (transport, _peer) = socket_transport();
        let timeout = Duration::from_micros(500);
        let started = Instant::now();
        let e = transport.read_input(&mut [0; 20], timeout).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() >= timeout);
    }

    #[test]
    fn output_is_one_complete_report() {
        let (transport, mut peer) = socket_transport();
        assert!(transport.write_output(&[]).is_err());
        assert_eq!(transport.write_output(&[19, 2, 3]).unwrap(), 3);
        let mut received = [0; 3];
        peer.read_exact(&mut received).unwrap();
        assert_eq!(received, [19, 2, 3]);
    }
}
