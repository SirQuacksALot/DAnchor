use std::net::UdpSocket;
use std::path::{Path, PathBuf};

use danchor_core::discovery::{DeviceInfo, MdnsAdvertiser, ServiceAdvertiser};
use danchor_core::protocol;
use danchor_core::security;
use danchor_core::transport::{self, ConnectionRegistry, DeviceIdentity};
use uuid::Uuid;

fn main() {
    let hostname = hostname::get()
        .expect("failed to read system hostname")
        .into_string()
        .expect("hostname is not valid UTF-8");

    let device_id = load_or_create_device_id();

    let socket =
        UdpSocket::bind(("0.0.0.0", transport::DEFAULT_PORT)).expect("failed to bind UDP socket");
    let port = socket
        .local_addr()
        .expect("socket has no local address")
        .port();

    let device = DeviceInfo {
        protocol_version: protocol::VERSION,
        device_id: device_id.clone(),
        device_name: hostname.clone(),
        device_icon: "desktop".to_string(),
    };
    let host_name = format!("{hostname}.local.");

    let mut advertiser = MdnsAdvertiser::new().expect("failed to start mDNS daemon");
    advertiser
        .advertise(&hostname, &host_name, port, &device)
        .expect("failed to advertise DAnchor service");

    println!(
        "DAnchor desktop \"{hostname}\" (id {device_id}) listening on UDP port {port}, advertised via mDNS as {host_name}"
    );

    let identity = DeviceIdentity {
        device_id: &device.device_id,
        device_name: &device.device_name,
        device_icon: &device.device_icon,
    };

    let noise_key = load_or_create_noise_key(&config_dir().join("noise_key"));
    let trust_secret = load_trust_secret(&config_dir().join("trust_secret"));
    if trust_secret.is_none() {
        println!(
            "no trust secret configured at ~/.config/danchor/trust_secret - PSK-authenticated connections will be rejected until one is added (copy the hex value from the Android app's Profile tab)"
        );
    }
    let mut registry = ConnectionRegistry::new(identity, noise_key, trust_secret);

    let mut buf = [0u8; 2048];
    loop {
        match registry.serve_one(&socket, &mut buf) {
            Ok(sender) => match registry.established_peer_fingerprint(&sender) {
                Some(fingerprint) => {
                    println!(
                        "handled datagram from {sender} (secure session, peer key {fingerprint}...)"
                    )
                }
                None => println!("handled datagram from {sender}"),
            },
            Err(err) => eprintln!("socket error: {err}"),
        }
    }
}

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable not set");
    let dir = PathBuf::from(home).join(".config").join("danchor");
    std::fs::create_dir_all(&dir).expect("failed to create config directory");
    dir
}

// This desktop's stable identity, independent of hostname/network address -
// generated once with a real UUID v4 (matching the format the Android app
// already generates its own device_id in) and persisted to a small file so
// it survives restarts, rather than a fresh random id every launch.
fn load_or_create_device_id() -> String {
    let id_path = config_dir().join("device_id");

    if let Ok(existing) = std::fs::read_to_string(&id_path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let generated = Uuid::new_v4().to_string();
    std::fs::write(&id_path, &generated).expect("failed to write device_id file");
    generated
}

// This desktop's stable Noise static keypair - `snow`'s XX-family patterns
// need one on both sides of a handshake regardless of PSK use (see
// conventions.toon). Stored as a raw byte file rather than hex since it's
// entirely machine-generated/opaque, never manually created or read by a
// person (unlike `trust_secret` below).
fn load_or_create_noise_key(path: &Path) -> Vec<u8> {
    if let Ok(existing) = std::fs::read(path)
        && !existing.is_empty()
    {
        return existing;
    }

    let keypair =
        security::generate_static_keypair().expect("failed to generate a Noise static keypair");
    std::fs::write(path, &keypair.private).expect("failed to write noise_key file");
    keypair.private
}

// The household trust secret Android's Profile tab already generates and
// displays (`AppPreferences.pairingSecret`, hex-encoded). Deliberately not
// auto-generated here - the desktop's copy has to match a value a person
// pastes in from that screen, so `None` (no PSK-authenticated connections
// possible yet) is the correct default until Module 1c/1d build a real way
// to configure it.
fn load_trust_secret(path: &Path) -> Option<[u8; security::PSK_LEN]> {
    let contents = std::fs::read_to_string(path).ok()?;
    let bytes = decode_hex(contents.trim())?;
    bytes.try_into().ok()
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
