//! The shared UDP socket of the Win-Test receiver and of `airscout-cli wintest-sim --listen`.

use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::{SocketAddr, UdpSocket};

/// `MapDlg.cs:7032`: UDP socket with address reuse, so several programs can listen on the port.
/// On Unix `SO_REUSEPORT` gives the shared-port effect that Windows `ReuseAddress` has.
/// The socket has no read timeout; the caller sets one.
pub fn bind_shared(addr: SocketAddr) -> io::Result<UdpSocket> {
    let sock = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    #[cfg(all(
        unix,
        not(any(
            target_os = "solaris",
            target_os = "illumos",
            target_os = "cygwin",
            target_os = "nuttx"
        ))
    ))]
    sock.set_reuse_port(true)?;
    sock.bind(&addr.into())?;
    Ok(sock.into())
}
