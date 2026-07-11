use std::io;
use std::net::SocketAddr;

use super::responder::{DeviceIdentity, handle_datagram};
use super::socket::DatagramSocket;

/// Services exactly one incoming datagram: receives it, decides on a reply
/// via `handle_datagram`, and sends the reply back to the sender if there
/// is one. Returns the sender's address regardless of whether a reply was
/// sent, so a caller can log/track activity.
///
/// Generic over `DatagramSocket` so this - the actual recv-decide-reply
/// cycle - is unit testable without a real socket; only the trait impl for
/// `std::net::UdpSocket` and the caller's loop around this function are
/// real, untested I/O.
pub fn serve_one<S: DatagramSocket>(
    socket: &S,
    buf: &mut [u8],
    identity: DeviceIdentity,
) -> io::Result<SocketAddr> {
    let (len, sender) = socket.recv_from(buf)?;
    if let Some(reply) = handle_datagram(&buf[..len], identity) {
        socket.send_to(&reply, sender)?;
    }
    Ok(sender)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Packet, PacketBody, PongInfo};
    use std::cell::RefCell;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct MockSocket {
        incoming: RefCell<VecDeque<(Vec<u8>, SocketAddr)>>,
        sent: RefCell<Vec<(Vec<u8>, SocketAddr)>>,
    }

    impl DatagramSocket for MockSocket {
        fn send_to(&self, buf: &[u8], addr: SocketAddr) -> io::Result<usize> {
            self.sent.borrow_mut().push((buf.to_vec(), addr));
            Ok(buf.len())
        }

        fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
            let (data, addr) = self
                .incoming
                .borrow_mut()
                .pop_front()
                .expect("test datagram queue exhausted");
            buf[..data.len()].copy_from_slice(&data);
            Ok((data.len(), addr))
        }
    }

    fn addr() -> SocketAddr {
        "192.168.1.50:9999".parse().unwrap()
    }

    fn identity() -> DeviceIdentity<'static> {
        DeviceIdentity {
            device_id: "550e8400-e29b-41d4-a716-446655440000",
            device_name: "My Desktop",
            device_icon: "desktop",
        }
    }

    #[test]
    fn serves_a_ping_with_a_pong_reply() {
        let ping = Packet {
            sequence: 1,
            body: PacketBody::Ping(42),
        }
        .encode()
        .unwrap();

        let socket = MockSocket::default();
        socket.incoming.borrow_mut().push_back((ping, addr()));

        let mut buf = [0u8; 1024];
        let sender = serve_one(&socket, &mut buf, identity()).unwrap();
        assert_eq!(sender, addr());

        let sent = socket.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1, addr());
        let reply = Packet::decode(&sent[0].0).unwrap();
        assert_eq!(
            reply.body,
            PacketBody::Pong(PongInfo {
                timestamp_ms: 42,
                device_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                device_name: "My Desktop".to_string(),
                device_icon: "desktop".to_string(),
            })
        );
    }

    #[test]
    fn serving_a_packet_with_no_reply_sends_nothing() {
        let pong = Packet {
            sequence: 1,
            body: PacketBody::Pong(PongInfo {
                timestamp_ms: 1,
                device_id: String::new(),
                device_name: String::new(),
                device_icon: String::new(),
            }),
        }
        .encode()
        .unwrap();

        let socket = MockSocket::default();
        socket.incoming.borrow_mut().push_back((pong, addr()));

        let mut buf = [0u8; 1024];
        let sender = serve_one(&socket, &mut buf, identity()).unwrap();
        assert_eq!(sender, addr());
        assert!(socket.sent.borrow().is_empty());
    }
}
