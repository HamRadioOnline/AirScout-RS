//! Interface addresses the UDP sockets use (`NetworkHelper` in the C#).

use std::net::Ipv4Addr;

/// Local address to bind and address to broadcast to. Tests use [`NetAddrs::loopback`] so no
/// packet leaves the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetAddrs {
    pub bind_ip: Ipv4Addr,
    pub broadcast_ip: Ipv4Addr,
}

impl Default for NetAddrs {
    /// `0.0.0.0` and `255.255.255.255`, as upstream.
    fn default() -> Self {
        Self {
            bind_ip: Ipv4Addr::UNSPECIFIED,
            broadcast_ip: Ipv4Addr::BROADCAST,
        }
    }
}

impl NetAddrs {
    /// `127.0.0.1` for both, for tests.
    pub fn loopback() -> Self {
        Self {
            bind_ip: Ipv4Addr::LOCALHOST,
            broadcast_ip: Ipv4Addr::LOCALHOST,
        }
    }
}
