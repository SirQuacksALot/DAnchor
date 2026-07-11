use std::io;
use std::net::SocketAddr;

/// Abstraction over "a thing that can send/receive UDP datagrams to/from a
/// peer address" - mirrors `std::net::UdpSocket`'s own API exactly (both
/// methods take `&self`, matching how a real socket permits concurrent
/// send/recv without exclusive access), so the real backend is a one-line
/// delegation and anything built on this trait stays testable without
/// opening a real socket.
pub trait DatagramSocket {
    fn send_to(&self, buf: &[u8], addr: SocketAddr) -> io::Result<usize>;
    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)>;
}

impl DatagramSocket for std::net::UdpSocket {
    fn send_to(&self, buf: &[u8], addr: SocketAddr) -> io::Result<usize> {
        std::net::UdpSocket::send_to(self, buf, addr)
    }

    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        std::net::UdpSocket::recv_from(self, buf)
    }
}
