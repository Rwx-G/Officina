//! Address classes (AD-103 step 3, D-234): which resolved addresses the
//! gateway may connect to in direct mode.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::GatewayRefusal;

/// Non-public IPv4 ranges, as (network, prefix length). Source: IANA's
/// "IPv4 Special-Purpose Address Registry", every block not globally
/// reachable, with multicast (224.0.0.0/4), reserved (240.0.0.0/4) and the
/// limited broadcast address. Nothing else transcribes it: the vector test
/// derives its cases from this table.
pub const NON_PUBLIC_V4: &[(u32, u8)] = &[
    (0x0000_0000, 8),  // 0.0.0.0/8, "this network"
    (0x0A00_0000, 8),  // 10.0.0.0/8, private
    (0x6440_0000, 10), // 100.64.0.0/10, shared address space
    (0x7F00_0000, 8),  // 127.0.0.0/8, loopback
    (0xA9FE_0000, 16), // 169.254.0.0/16, link-local
    (0xAC10_0000, 12), // 172.16.0.0/12, private
    (0xC000_0000, 24), // 192.0.0.0/24, IETF protocol assignments
    (0xC000_0200, 24), // 192.0.2.0/24, documentation
    (0xC01F_C400, 24), // 192.31.196.0/24, AS112-v4
    (0xC034_C100, 24), // 192.52.193.0/24, AMT
    (0xC058_6300, 24), // 192.88.99.0/24, deprecated 6to4 relay anycast
    (0xC0A8_0000, 16), // 192.168.0.0/16, private
    (0xC0AF_3000, 24), // 192.175.48.0/24, direct delegation AS112
    (0xC612_0000, 15), // 198.18.0.0/15, benchmarking
    (0xC633_6400, 24), // 198.51.100.0/24, documentation
    (0xCB00_7100, 24), // 203.0.113.0/24, documentation
    (0xE000_0000, 4),  // 224.0.0.0/4, multicast
    (0xF000_0000, 4),  // 240.0.0.0/4, reserved, and the broadcast address
];

/// Non-public IPv6 ranges, as (network, prefix length). Source: IANA's
/// "IPv6 Special-Purpose Address Registry", every block not globally
/// reachable, with multicast (ff00::/8). The forms that embed an IPv4
/// address and are public only when it is ([`EMBEDDING_V6`]) are not here.
pub const NON_PUBLIC_V6: &[(u128, u8)] = &[
    (0, 96), // ::/96, unspecified, loopback, IPv4-compatible
    (0x0064_ff9b_0001_0000_0000_0000_0000_0000, 48), // 64:ff9b:1::/48, local-use NAT64
    (0x0100_0000_0000_0000_0000_0000_0000_0000, 64), // 100::/64, discard-only
    (0x2001_0000_0000_0000_0000_0000_0000_0000, 23), // 2001::/23, IETF protocol assignments, Teredo
    (0x2001_0db8_0000_0000_0000_0000_0000_0000, 32), // 2001:db8::/32, documentation
    (0x3fff_0000_0000_0000_0000_0000_0000_0000, 20), // 3fff::/20, documentation
    (0x5f00_0000_0000_0000_0000_0000_0000_0000, 16), // 5f00::/16, segment routing SIDs
    (0xfc00_0000_0000_0000_0000_0000_0000_0000, 7), // fc00::/7, unique local
    (0xfe80_0000_0000_0000_0000_0000_0000_0000, 10), // fe80::/10, link-local
    (0xff00_0000_0000_0000_0000_0000_0000_0000, 8), // ff00::/8, multicast
];

/// An IPv6 form that carries an IPv4 address: its prefix, its length, and
/// the bit offset (from the most significant bit) of the IPv4 address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Embedding {
    pub network: u128,
    pub prefix: u8,
    pub ipv4_at_bit: u8,
}

/// The IPv6 forms whose embedded IPv4 address is checked (D-234): an
/// IPv4-mapped address, NAT64's well-known prefix and 6to4. Teredo
/// (2001::/32) sits inside 2001::/23, never public whatever it embeds.
pub const EMBEDDING_V6: &[Embedding] = &[
    Embedding {
        network: 0x0000_0000_0000_0000_0000_ffff_0000_0000,
        prefix: 96,
        ipv4_at_bit: 96,
    },
    Embedding {
        network: 0x0064_ff9b_0000_0000_0000_0000_0000_0000,
        prefix: 96,
        ipv4_at_bit: 96,
    },
    Embedding {
        network: 0x2002_0000_0000_0000_0000_0000_0000_0000,
        prefix: 16,
        ipv4_at_bit: 16,
    },
];

/// The mask of a `prefix`-bit network in a `bits`-bit address.
fn mask(prefix: u8, bits: u32) -> u128 {
    if prefix == 0 {
        0
    } else {
        (u128::MAX << (128 - u32::from(prefix))) >> (128 - bits)
    }
}

fn in_v4(addr: u32, (network, prefix): (u32, u8)) -> bool {
    let m = u32::try_from(mask(prefix, 32)).unwrap_or(u32::MAX);
    addr & m == network & m
}

fn in_v6(addr: u128, network: u128, prefix: u8) -> bool {
    let m = mask(prefix, 128);
    addr & m == network & m
}

/// The IPv4 address an IPv6 address embeds, when it is one of
/// [`EMBEDDING_V6`]'s forms.
pub fn embedded_ipv4(addr: Ipv6Addr) -> Option<Ipv4Addr> {
    let bits = u128::from(addr);
    EMBEDDING_V6
        .iter()
        .find(|form| in_v6(bits, form.network, form.prefix))
        .map(|form| {
            let shift = 128 - 32 - u32::from(form.ipv4_at_bit);
            Ipv4Addr::from(u32::try_from((bits >> shift) & 0xFFFF_FFFF).unwrap_or(0))
        })
}

/// True when the gateway may connect to `addr` in direct mode.
pub fn is_public(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => {
            let bits = u32::from(v4);
            !NON_PUBLIC_V4.iter().any(|range| in_v4(bits, *range))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = embedded_ipv4(v6) {
                return is_public(IpAddr::V4(v4));
            }
            let bits = u128::from(v6);
            !NON_PUBLIC_V6
                .iter()
                .any(|(network, prefix)| in_v6(bits, *network, *prefix))
        }
    }
}

/// The address the gateway connects to among `addrs`, a name's resolution
/// (AD-103 step 3, D-234): every address is checked, any non-public one
/// refuses `private_address`, none in IPv4 refuses `no_ipv4`, otherwise the
/// first IPv4 address.
pub fn choose(addrs: &[IpAddr]) -> Result<Ipv4Addr, GatewayRefusal> {
    if addrs.iter().any(|addr| !is_public(*addr)) {
        return Err(GatewayRefusal::PrivateAddress);
    }
    addrs
        .iter()
        .find_map(|addr| match addr {
            IpAddr::V4(v4) => Some(*v4),
            IpAddr::V6(_) => None,
        })
        .ok_or(GatewayRefusal::NoIpv4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_table_entry_is_a_network_address() {
        for (network, prefix) in NON_PUBLIC_V4 {
            let m = u32::try_from(mask(*prefix, 32)).unwrap();
            assert_eq!(network & !m, 0, "{network:#x}/{prefix}");
        }
        for (network, prefix) in NON_PUBLIC_V6 {
            assert_eq!(network & !mask(*prefix, 128), 0, "{network:#x}/{prefix}");
        }
    }

    #[test]
    fn a_public_address_passes() {
        assert!(is_public("160.79.104.10".parse().unwrap()));
        assert!(is_public("2607:6bc0::10".parse().unwrap()));
        assert!(!is_public("::ffff:127.0.0.1".parse().unwrap()));
        assert!(is_public("::ffff:160.79.104.10".parse().unwrap()));
    }
}
