mod capture;

use std::net::UdpSocket;
use std::path::{Path, PathBuf};

use danchor_core::discovery::{DeviceInfo, MdnsAdvertiser, ServiceAdvertiser};
use danchor_core::protocol;
use danchor_core::security;
use danchor_core::transport::{self, ConnectionRegistry, DeviceIdentity};
use uuid::Uuid;

fn main() {
    // Temporary verification hook for Module 2a (screen capture+encode,
    // standalone - not yet wired into the network transport). Remove once
    // Module 2b wires captured frames into a real streaming session.
    if std::env::args().any(|arg| arg == "--capture-test") {
        run_capture_test();
        return;
    }

    // Temporary verification hook for Module 2b: streams real captured
    // frames live, over the network, to a tablet that's already completed a
    // secure handshake with this desktop. Remove once this becomes the
    // normal serve-loop behavior instead of a separate opt-in test mode.
    if let Some(pos) = std::env::args().position(|arg| arg == "--mirror-test") {
        // Just an IP, not ip:port - the tablet's handshake arrives from a
        // fresh ephemeral source port each run, so the exact port can't be
        // known ahead of time; matching on IP alone is fine for this
        // single-device test.
        let target_ip: std::net::IpAddr = std::env::args()
            .nth(pos + 1)
            .expect("--mirror-test needs a tablet IP argument")
            .parse()
            .expect("invalid IP for --mirror-test");
        run_mirror_test(target_ip);
        return;
    }

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

// Standalone verification for Module 2a: captures the real screen for a
// fixed window, writing raw encoded H.264 to a file so it can be played
// back and visually confirmed (e.g. `ffplay /tmp/danchor-capture-test.h264`).
// Also dumps each access unit as its own numbered file under
// /tmp/danchor-capture-frames/ - a flat concatenated .h264 loses frame
// boundaries, but the Android MediaCodec decode test (Module 2b groundwork)
// needs to feed exactly one access unit per input buffer, so per-frame files
// are the simplest way to hand that over without writing a NAL-unit parser.
fn run_capture_test() {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    gstreamer::init().expect("gstreamer init failed");

    let out_path = "/tmp/danchor-capture-test.h264";
    let frames_dir = PathBuf::from("/tmp/danchor-capture-frames");
    std::fs::remove_dir_all(&frames_dir).ok();
    std::fs::create_dir_all(&frames_dir).expect("failed to create capture frames directory");

    let file = Arc::new(Mutex::new(
        std::fs::File::create(out_path).expect("failed to create capture output file"),
    ));
    let frame_count = Arc::new(AtomicU64::new(0));

    let file_for_callback = file.clone();
    let frame_count_for_callback = frame_count.clone();
    let frames_dir_for_callback = frames_dir.clone();
    let session = capture::CaptureSession::start(move |frame| {
        let n = frame_count_for_callback.fetch_add(1, Ordering::Relaxed) + 1;
        if n <= 5 || n.is_multiple_of(30) {
            println!(
                "frame {n}: {} bytes, keyframe={}",
                frame.data.len(),
                frame.keyframe
            );
        }
        let _ = file_for_callback.lock().unwrap().write_all(&frame.data);
        let frame_path = frames_dir_for_callback.join(format!("frame_{n:05}.h264"));
        let _ = std::fs::write(frame_path, &frame.data);
    })
    .expect("failed to start screen capture");

    println!("capturing for 8 seconds...");
    std::thread::sleep(std::time::Duration::from_secs(8));
    session.stop();

    println!(
        "done - {} frames written to {out_path} and to {}/",
        frame_count.load(Ordering::Relaxed),
        frames_dir.display()
    );
}

// Standalone verification for Module 2b: waits for a specific tablet to
// complete a secure handshake (e.g. by tapping its saved connection in the
// Android app), then captures the real screen and streams each encoded
// frame to it live, fragmented via the existing protocol::fragment_frame
// and encrypted via the same per-peer session transport::ConnectionRegistry
// already tracks - proving the full live network path this module was
// missing (Module 2a proved capture+encode standalone; the Android side's
// MediaCodec decode+render was proven separately against local files).
fn run_mirror_test(target_ip: std::net::IpAddr) {
    use std::sync::{Arc, Mutex};

    gstreamer::init().expect("gstreamer init failed");

    let hostname = hostname::get()
        .expect("failed to read system hostname")
        .into_string()
        .expect("hostname is not valid UTF-8");
    let device_id = load_or_create_device_id();
    // Leaked deliberately: this is a short-lived opt-in test binary path (not
    // the real serve loop), and `ConnectionRegistry`'s borrowed
    // `DeviceIdentity` needs to be movable into the 'static capture callback
    // below - a real fix (owned Strings in DeviceIdentity/ConnectionRegistry)
    // belongs with turning this test mode into the actual serve-loop
    // behavior, not this throwaway verification pass.
    let device: &'static DeviceInfo = Box::leak(Box::new(DeviceInfo {
        protocol_version: protocol::VERSION,
        device_id,
        device_name: hostname,
        device_icon: "desktop".to_string(),
    }));
    let identity = DeviceIdentity {
        device_id: &device.device_id,
        device_name: &device.device_name,
        device_icon: &device.device_icon,
    };

    let socket = Arc::new(
        UdpSocket::bind(("0.0.0.0", transport::DEFAULT_PORT)).expect("failed to bind UDP socket"),
    );
    let noise_key = load_or_create_noise_key(&config_dir().join("noise_key"));
    let trust_secret = load_trust_secret(&config_dir().join("trust_secret"));
    let registry = Arc::new(Mutex::new(ConnectionRegistry::new(
        identity,
        noise_key,
        trust_secret,
    )));

    println!(
        "waiting for a secure handshake from {target_ip} (tap its saved connection in the Android app)..."
    );
    let mut buf = [0u8; 2048];
    let target = loop {
        let sender = registry
            .lock()
            .unwrap()
            .serve_one(&*socket, &mut buf)
            .expect("socket error while waiting for handshake");
        if sender.ip() == target_ip
            && registry
                .lock()
                .unwrap()
                .established_peer_fingerprint(&sender)
                .is_some()
        {
            break sender;
        }
    };
    println!("secure session established with {target} - starting capture and streaming...");

    let socket_for_capture = socket.clone();
    let registry_for_capture = registry.clone();
    let mut next_frame_id: u32 = 0;
    let mut sent_count: u64 = 0;
    let session = capture::CaptureSession::start(move |frame| {
        next_frame_id = next_frame_id.wrapping_add(1);
        let Ok(fragments) =
            protocol::fragment_frame(next_frame_id, &frame.data, frame.keyframe, 1200)
        else {
            return;
        };
        for fragment in fragments {
            let body = protocol::PacketBody::Video(fragment);
            let Some(bytes) = registry_for_capture
                .lock()
                .unwrap()
                .send_to_established(&target, body)
            else {
                continue;
            };
            let _ = socket_for_capture.send_to(&bytes, target);
        }
        sent_count += 1;
        if sent_count.is_multiple_of(30) {
            println!("streamed frame {sent_count}");
        }
    })
    .expect("failed to start screen capture");

    println!("mirroring for 30 seconds - watch the tablet...");
    std::thread::sleep(std::time::Duration::from_secs(30));
    session.stop();
    println!("done");
}
