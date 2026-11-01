//! The gateway's vector set (story 6.5 AC 2 to 5, AD-103, D-234):
//! `tests/vectors/gateway.json` replayed against core's rules; the address
//! ranges' cases derived from `addr.rs`'s tables, never transcribed.

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use officina_core::workshop::gateway::addr::{
    EMBEDDING_V6, NON_PUBLIC_V4, NON_PUBLIC_V6, choose, embedded_ipv4, is_public,
};
use officina_core::workshop::gateway::tls::{HelloRead, server_name};
use officina_core::workshop::gateway::{
    GatewayLine, GatewayRefusal, Request, RequestRead, admit, parse_request,
};
use serde_json::Value;

const VECTORS: &str = include_str!("../../../tests/vectors/gateway.json");

fn vectors() -> Value {
    serde_json::from_str(VECTORS).unwrap()
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn refusal(value: &Value) -> Option<GatewayRefusal> {
    value
        .get("refused")
        .and_then(Value::as_str)
        .map(|code| GatewayRefusal::from_code(code).unwrap_or_else(|| panic!("{code}")))
}

fn request_bytes(case: &Value) -> Vec<u8> {
    if let Some(text) = case["text"].as_str() {
        return text.as_bytes().to_vec();
    }
    if let Some(h) = case["hex"].as_str() {
        return hex(h);
    }
    let repeat = &case["repeat"];
    let byte = repeat["byte"].as_str().unwrap().as_bytes()[0];
    vec![byte; usize::try_from(repeat["count"].as_u64().unwrap()).unwrap()]
}

#[test]
fn request_cases_match() {
    for case in vectors()["requests"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let got = parse_request(&request_bytes(case));
        let expect = &case["expect"];
        let wanted = if expect == "more" {
            RequestRead::More
        } else if let Some(cause) = refusal(expect) {
            RequestRead::Refused(cause)
        } else {
            RequestRead::Done(Request {
                host: expect["host"].as_str().unwrap().to_owned(),
                port: u16::try_from(expect["port"].as_u64().unwrap()).unwrap(),
                rest: hex(expect["rest_hex"].as_str().unwrap()),
            })
        };
        assert_eq!(got, wanted, "{name}");
    }
}

#[test]
fn host_cases_match() {
    let v = vectors();
    let allow: Vec<String> = v["allow"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_owned())
        .collect();
    for case in v["hosts"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let request = Request {
            host: case["host"].as_str().unwrap().to_owned(),
            port: u16::try_from(case["port"].as_u64().unwrap()).unwrap(),
            rest: Vec::new(),
        };
        let got = admit(&request, &allow);
        let expect = &case["expect"];
        match refusal(expect) {
            Some(cause) => assert_eq!(got.err(), Some(cause), "{name}"),
            None => {
                let admitted = got.unwrap_or_else(|e| panic!("{name}: {e:?}"));
                assert_eq!(admitted.host, expect["host"].as_str().unwrap(), "{name}");
                assert_eq!(admitted.entry, expect["entry"].as_str().unwrap(), "{name}");
            }
        }
    }
}

/// Every refusal is decided by a case of the set, or named in its
/// `program` section with the program test that proves it.
#[test]
fn every_refusal_has_a_case() {
    let v = vectors();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for section in ["requests", "hosts", "client_hello"] {
        for case in v[section].as_array().unwrap() {
            if let Some(code) = case["expect"]["refused"].as_str() {
                seen.insert(code);
            }
        }
    }
    for case in v["addresses"]["choose"].as_array().unwrap() {
        if let Some(code) = case["expect"]["refused"].as_str() {
            seen.insert(code);
        }
    }
    for case in v["program"].as_array().unwrap() {
        seen.insert(case["cause"].as_str().unwrap());
    }
    let all: BTreeSet<&str> = GatewayRefusal::ALL.iter().map(|c| c.code()).collect();
    assert_eq!(seen, all);
}

/// The inside, first, last and just-outside addresses of a `prefix`-bit
/// network of `bits`-bit addresses.
fn edges(network: u128, prefix: u8, bits: u32) -> (u128, u128, u128, Option<u128>, Option<u128>) {
    let size: u128 = if u32::from(prefix) == bits {
        1
    } else {
        1u128 << (bits - u32::from(prefix))
    };
    let first = network;
    let last = network + (size - 1);
    let inside = network + size / 2;
    let max = if bits == 128 {
        u128::MAX
    } else {
        (1u128 << bits) - 1
    };
    (
        inside,
        first,
        last,
        first.checked_sub(1),
        (last < max).then(|| last + 1),
    )
}

fn v4(n: u128) -> IpAddr {
    IpAddr::V4(Ipv4Addr::from(u32::try_from(n).unwrap()))
}

fn v6(n: u128) -> IpAddr {
    IpAddr::V6(Ipv6Addr::from(n))
}

/// True when `addr` lies in one of the non-public tables or embedding
/// forms: the oracle for the address just outside a range, which may sit
/// in a neighbouring range.
fn listed(addr: IpAddr) -> bool {
    !is_public(addr)
}

#[test]
fn every_range_has_a_case_inside_and_at_each_edge() {
    for (network, prefix) in NON_PUBLIC_V4 {
        let (inside, first, last, before, after) = edges(u128::from(*network), *prefix, 32);
        for addr in [inside, first, last] {
            assert!(
                !is_public(v4(addr)),
                "{} in {network:#x}/{prefix}",
                v4(addr)
            );
        }
        for outside in [before, after].into_iter().flatten() {
            let addr = v4(outside);
            let neighbour = NON_PUBLIC_V4.iter().any(|(n, p)| {
                (*n, *p) != (*network, *prefix) && {
                    let (_, f, l, _, _) = edges(u128::from(*n), *p, 32);
                    (f..=l).contains(&outside)
                }
            });
            assert_eq!(
                listed(addr),
                neighbour,
                "{addr} next to {network:#x}/{prefix}"
            );
        }
    }
    for (network, prefix) in NON_PUBLIC_V6 {
        let (inside, first, last, before, after) = edges(*network, *prefix, 128);
        for addr in [inside, first, last] {
            assert!(
                !is_public(v6(addr)),
                "{} in {network:#x}/{prefix}",
                v6(addr)
            );
        }
        for outside in [before, after].into_iter().flatten() {
            let addr = v6(outside);
            let neighbour = NON_PUBLIC_V6.iter().any(|(n, p)| {
                (*n, *p) != (*network, *prefix) && {
                    let (_, f, l, _, _) = edges(*n, *p, 128);
                    (f..=l).contains(&outside)
                }
            });
            let embeds = match addr {
                IpAddr::V6(a) => embedded_ipv4(a).is_some(),
                IpAddr::V4(_) => false,
            };
            if !embeds {
                assert_eq!(
                    listed(addr),
                    neighbour,
                    "{addr} next to {network:#x}/{prefix}"
                );
            }
        }
    }
}

#[test]
fn embedded_ipv4_forms_are_unwrapped() {
    for form in EMBEDDING_V6 {
        let shift = 128 - 32 - u32::from(form.ipv4_at_bit);
        let private = form.network | (u128::from(u32::from(Ipv4Addr::new(10, 0, 0, 1))) << shift);
        let public =
            form.network | (u128::from(u32::from(Ipv4Addr::new(160, 79, 104, 10))) << shift);
        assert_eq!(
            embedded_ipv4(Ipv6Addr::from(private)),
            Some(Ipv4Addr::new(10, 0, 0, 1))
        );
        assert!(!is_public(v6(private)));
        assert!(is_public(v6(public)), "{}", v6(public));
    }
    for case in vectors()["addresses"]["embedded"].as_array().unwrap() {
        let addr: IpAddr = case["addr"].as_str().unwrap().parse().unwrap();
        assert_eq!(is_public(addr), case["public"].as_bool().unwrap(), "{addr}");
    }
}

#[test]
fn the_first_ipv4_address_is_chosen() {
    for case in vectors()["addresses"]["choose"].as_array().unwrap() {
        let addrs: Vec<IpAddr> = case["addrs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().parse().unwrap())
            .collect();
        let expect = &case["expect"];
        let got = choose(&addrs);
        match refusal(expect) {
            Some(cause) => assert_eq!(got, Err(cause), "{addrs:?}"),
            None => assert_eq!(
                got,
                Ok(expect["ipv4"].as_str().unwrap().parse().unwrap()),
                "{addrs:?}"
            ),
        }
    }
}

#[test]
fn client_hello_cases_match() {
    for case in vectors()["client_hello"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let got = server_name(&hex(case["hex"].as_str().unwrap()));
        let expect = &case["expect"];
        if expect == "more" {
            assert_eq!(got, HelloRead::More, "{name}");
        } else if let Some(cause) = refusal(expect) {
            assert_eq!(got, HelloRead::Refused(cause), "{name}");
        } else {
            let HelloRead::Done(hello) = got else {
                panic!("{name}: {got:?}")
            };
            assert_eq!(
                hello.server_name,
                expect["server_name"].as_str().unwrap(),
                "{name}"
            );
            assert_eq!(hello.ech, expect["ech"].as_bool().unwrap(), "{name}");
        }
    }
}

#[test]
fn line_cases_match() {
    for case in vectors()["lines"].as_array().unwrap() {
        let line: GatewayLine = serde_json::from_value(case["line"].clone()).unwrap();
        assert_eq!(line.to_line(), case["text"].as_str().unwrap());
    }
}
