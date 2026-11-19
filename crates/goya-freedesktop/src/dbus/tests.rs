use super::testutil::*;
use super::wire::{frame_length, parse_signature};
use super::*;
use std::thread;

fn roundtrip(sig: &str, vals: &[Value]) -> Vec<u8> {
    let bytes = marshal_body(sig, vals).unwrap();
    let back = unmarshal_body(sig, &bytes).unwrap();
    assert_eq!(back, vals, "round-trip of {sig}");
    bytes
}

#[test]
fn basic_types_round_trip() {
    roundtrip(
        "ybnqiuxtdsog",
        &[
            Value::Byte(0xab),
            Value::Bool(true),
            Value::Int16(-2),
            Value::Uint16(65535),
            Value::Int32(-70000),
            Value::Uint32(0xdead_beef),
            Value::Int64(i64::MIN),
            Value::Uint64(u64::MAX),
            Value::Double(0.816),
            Value::str("héllo"),
            Value::ObjectPath("/org/freedesktop/portal/desktop".into()),
            Value::Signature("a{sv}(ddd)".into()),
        ],
    );
}

#[test]
fn containers_round_trip() {
    let dict = Value::vardict([
        ("urgency", Value::Byte(2)),
        ("category", Value::str("im.received")),
        (
            "nested",
            Value::variant(Value::Struct(vec![
                Value::Double(0.388),
                Value::Double(0.816),
                Value::Double(0.875),
            ])),
        ),
        ("list", Value::string_array(["a", "", "c"])),
    ]);
    roundtrip(
        "a{sv}aasa(yv)",
        &[
            dict,
            Value::Array {
                elem_sig: "as".into(),
                items: vec![
                    Value::string_array(Vec::<String>::new()),
                    Value::string_array(["x"]),
                ],
            },
            Value::Array {
                elem_sig: "(yv)".into(),
                items: vec![],
            },
        ],
    );
}

#[test]
fn bytes_are_little_endian() {
    assert_eq!(
        marshal_body("u", &[Value::Uint32(1)]).unwrap(),
        [1, 0, 0, 0]
    );
    assert_eq!(
        marshal_body("s", &[Value::str("ab")]).unwrap(),
        [2, 0, 0, 0, b'a', b'b', 0]
    );
    assert_eq!(
        marshal_body("g", &[Value::Signature("u".into())]).unwrap(),
        [1, b'u', 0]
    );
    assert_eq!(
        marshal_body("b", &[Value::Bool(true)]).unwrap(),
        [1, 0, 0, 0]
    );
    assert_eq!(
        marshal_body("v", &[Value::variant(Value::Byte(9))]).unwrap(),
        [1, b'y', 0, 9]
    );
}

#[test]
fn struct_is_8_aligned() {
    let b = roundtrip(
        "y(i)",
        &[Value::Byte(1), Value::Struct(vec![Value::Int32(-1)])],
    );
    assert_eq!(b, [1, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff]);
    let b = roundtrip("yd", &[Value::Byte(1), Value::Double(1.0)]);
    assert_eq!(b.len(), 16);
    assert_eq!(&b[1..8], &[0; 7]);
}

#[test]
fn empty_array_has_padding_but_zero_length() {
    // a{sv}: length word, then padding to the 8-aligned first element.
    let b = roundtrip("a{sv}", &[Value::vardict(Vec::<(String, Value)>::new())]);
    assert_eq!(b, [0, 0, 0, 0, 0, 0, 0, 0]);
    // After a byte: pad to 4 for the length, then to 8 for elements.
    let b = roundtrip(
        "ya(i)",
        &[
            Value::Byte(7),
            Value::Array {
                elem_sig: "(i)".into(),
                items: vec![],
            },
        ],
    );
    assert_eq!(b, [7, 0, 0, 0, 0, 0, 0, 0]);
    // Length word ends at an 8-aligned offset: no padding needed.
    let b = roundtrip(
        "uax",
        &[
            Value::Uint32(5),
            Value::Array {
                elem_sig: "x".into(),
                items: vec![],
            },
        ],
    );
    assert_eq!(b, [5, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn array_length_excludes_leading_padding() {
    let b = roundtrip(
        "a(y)",
        &[Value::Array {
            elem_sig: "(y)".into(),
            items: vec![Value::Struct(vec![Value::Byte(7)])],
        }],
    );
    assert_eq!(b, [1, 0, 0, 0, 0, 0, 0, 0, 7]);
    let b = roundtrip(
        "ax",
        &[Value::Array {
            elem_sig: "x".into(),
            items: vec![Value::Int64(1), Value::Int64(2)],
        }],
    );
    assert_eq!(&b[..4], &16u32.to_le_bytes());
    assert_eq!(b.len(), 24);
}

#[test]
fn dict_entries_are_8_aligned_and_counted_between() {
    let d = Value::Dict {
        key_sig: "y".into(),
        value_sig: "y".into(),
        entries: vec![
            (Value::Byte(1), Value::Byte(2)),
            (Value::Byte(3), Value::Byte(4)),
        ],
    };
    let b = roundtrip("a{yy}", &[d]);
    // len=10: entry(2) + padding to next entry(6) + entry(2)
    assert_eq!(b, [10, 0, 0, 0, 0, 0, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0, 3, 4]);
}

#[test]
fn variant_inner_value_is_aligned_to_its_type() {
    let b = roundtrip("v", &[Value::variant(Value::Uint32(3))]);
    assert_eq!(b, [1, b'u', 0, 0, 3, 0, 0, 0]);
}

#[test]
fn signatures_validated() {
    for bad in [
        "(", ")", "()", "a", "{sv}", "a{vs}", "a{s}", "a{sss}", "h", "z", "(s",
    ] {
        assert!(parse_signature(bad).is_err(), "{bad} should be rejected");
    }
    let deep = "a".repeat(33) + "y";
    assert!(parse_signature(&deep).is_err());
    let deep_struct = "(".repeat(33) + "y" + &")".repeat(33);
    assert!(parse_signature(&deep_struct).is_err());
    for good in [
        "",
        "y",
        "a{sv}",
        "(ddd)",
        "susssasa{sv}i",
        "a(yv)",
        "aaa{s(ii)}",
    ] {
        assert!(parse_signature(good).is_ok(), "{good} should be accepted");
    }
}

#[test]
fn encode_rejects_mismatches() {
    assert!(matches!(
        marshal_body("u", &[Value::Int32(1)]),
        Err(Error::TypeMismatch { .. })
    ));
    assert!(matches!(
        marshal_body("uu", &[Value::Uint32(1)]),
        Err(Error::InvalidArgument(_))
    ));
    assert!(matches!(
        marshal_body(
            "as",
            &[Value::Array {
                elem_sig: "u".into(),
                items: vec![]
            }]
        ),
        Err(Error::TypeMismatch { .. })
    ));
    assert!(marshal_body("s", &[Value::str("a\0b")]).is_err());
    assert!(marshal_body("o", &[Value::ObjectPath("no/slash".into())]).is_err());
}

#[test]
fn decode_rejects_bad_bytes() {
    assert!(unmarshal_body("b", &[2, 0, 0, 0]).is_err());
    assert!(unmarshal_body("s", &[2, 0, 0, 0, b'a', b'b']).is_err());
    assert!(unmarshal_body("s", &[2, 0, 0, 0, b'a', b'b', 1]).is_err());
    assert!(unmarshal_body("s", &[2, 0, 0, 0, 0xff, 0xfe, 0]).is_err());
    assert!(
        unmarshal_body("yu", &[1, 9, 0, 0, 1, 0, 0, 0]).is_err(),
        "non-zero padding"
    );
    assert!(
        unmarshal_body("u", &[1, 0, 0, 0, 0]).is_err(),
        "trailing bytes"
    );
    assert!(
        unmarshal_body("au", &[0xff, 0xff, 0xff, 0x00]).is_err(),
        "huge array"
    );
    assert!(
        unmarshal_body("au", &[8, 0, 0, 0, 1, 0, 0, 0]).is_err(),
        "array past end"
    );
    // Array element overruns the declared length.
    assert!(unmarshal_body("ay", &[0, 0, 0, 0, 5]).is_err());
    assert!(unmarshal_body("au", &[2, 0, 0, 0, 1, 0, 0, 0]).is_err());
}

#[test]
fn deeply_nested_variants_fail_cleanly() {
    // 10 000 nested `v` layers would overflow the stack if unbounded.
    let mut bytes = Vec::new();
    for _ in 0..10_000 {
        bytes.extend_from_slice(&[1, b'v', 0]);
    }
    bytes.extend_from_slice(&[1, b'y', 0, 1]);
    assert!(matches!(
        unmarshal_body("v", &bytes),
        Err(Error::Malformed { .. })
    ));
}

#[test]
fn many_tiny_values_are_capped() {
    // An `ay` of 1 MiB would be ~1M values; the cap rejects it.
    let n = 1usize << 20;
    let mut bytes = (n as u32).to_le_bytes().to_vec();
    bytes.resize(4 + n, 0);
    assert!(matches!(
        unmarshal_body("ay", &bytes),
        Err(Error::Malformed { .. })
    ));
}

fn sample_reply() -> Vec<u8> {
    let mut h = Header::new(MessageKind::MethodReturn, 7);
    h.reply_serial = Some(3);
    h.destination = Some(":1.42".into());
    h.sender = Some(":1.5".into());
    h.signature = "ssssasa{sv}(ddd)u".into();
    encode_message(
        &h,
        &[
            Value::str("cosmic-notifications"),
            Value::str("System76"),
            Value::str("0.1.0"),
            Value::str("1.2"),
            Value::string_array(["body", "actions", "body-markup"]),
            Value::vardict([
                ("color-scheme", Value::Uint32(1)),
                ("contrast", Value::Uint32(0)),
            ]),
            Value::Struct(vec![
                Value::Double(0.388),
                Value::Double(0.816),
                Value::Double(0.875),
            ]),
            Value::Uint32(1),
        ],
    )
    .unwrap()
}

#[test]
fn message_round_trip() {
    let bytes = sample_reply();
    assert_eq!(&bytes[..4], &[b'l', 2, 0, 1]);
    assert_eq!(frame_length(&bytes).unwrap(), bytes.len());
    let (h, body) = decode_message(&bytes).unwrap();
    assert_eq!(h.kind, MessageKind::MethodReturn);
    assert_eq!(h.serial, 7);
    assert_eq!(h.reply_serial, Some(3));
    assert_eq!(h.sender.as_deref(), Some(":1.5"));
    assert_eq!(body.len(), 8);
    assert_eq!(body[0].as_str(), Some("cosmic-notifications"));
}

#[test]
fn hello_call_layout_matches_reference() {
    // Hello call, serial 1, laid out per the specification.
    let mut h = Header::new(MessageKind::MethodCall, 1);
    h.path = Some("/org/freedesktop/DBus".into());
    h.interface = Some("org.freedesktop.DBus".into());
    h.member = Some("Hello".into());
    h.destination = Some("org.freedesktop.DBus".into());
    let b = encode_message(&h, &[]).unwrap();
    assert_eq!(&b[..12], &[b'l', 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0]);
    // fields: path(1) o, interface(2) s, member(3) s, destination(6) s
    assert_eq!(&b[16..24], &[1, 1, b'o', 0, 21, 0, 0, 0]);
    assert_eq!(b.len() % 8, 0);
    let (back, body) = decode_message(&b).unwrap();
    assert_eq!(back, h);
    assert!(body.is_empty());
}

#[test]
fn big_endian_message_decodes() {
    // Method return, serial 2, reply_serial 1, signature "u", body 0x01020304.
    let mut m = vec![b'B', 2, 0, 1, 0, 0, 0, 4, 0, 0, 0, 2];
    let fields: Vec<u8> = vec![
        5, 1, b'u', 0, 0, 0, 0, 1, // reply serial
        8, 1, b'g', 0, 1, b'u', 0, // signature
    ];
    m.extend_from_slice(&(fields.len() as u32).to_be_bytes());
    m.extend_from_slice(&fields);
    while !m.len().is_multiple_of(8) {
        m.push(0);
    }
    m.extend_from_slice(&[1, 2, 3, 4]);
    let (h, body) = decode_message(&m).unwrap();
    assert_eq!(h.reply_serial, Some(1));
    assert_eq!(body, vec![Value::Uint32(0x0102_0304)]);
}

#[test]
fn frame_length_caps_size() {
    let mut m = [0u8; 16];
    m[0] = b'l';
    m[4..8].copy_from_slice(&(2u32 << 20).to_le_bytes());
    assert!(matches!(frame_length(&m), Err(Error::MessageTooLarge(_))));
    m[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    m[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(frame_length(&m), Err(Error::MessageTooLarge(_))));
    m[0] = b'x';
    assert!(frame_length(&m).is_err());
}

#[test]
fn fuzz_truncation_never_panics() {
    let bytes = sample_reply();
    for n in 0..bytes.len() {
        assert!(decode_message(&bytes[..n]).is_err(), "prefix {n} accepted");
        let _ = frame_length(&bytes[..n]);
    }
    // Body-level truncation with a fixed signature.
    let (h, _) = decode_message(&bytes).unwrap();
    let body = marshal_body(&h.signature, &decode_message(&bytes).unwrap().1).unwrap();
    for n in 0..body.len() {
        assert!(unmarshal_body(&h.signature, &body[..n]).is_err());
    }
}

#[test]
fn fuzz_bit_flips_never_panic() {
    let bytes = sample_reply();
    let mut accepted = 0;
    for i in 0..bytes.len() * 8 {
        let mut m = bytes.clone();
        m[i / 8] ^= 1 << (i % 8);
        if decode_message(&m).is_ok() {
            accepted += 1;
        }
    }
    // Some flips (e.g. inside string contents) stay valid; most must not.
    assert!(accepted < bytes.len() * 8);
}

#[test]
fn fuzz_random_mutations_never_panic() {
    let base = sample_reply();
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut rnd = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..20_000 {
        let mut m = base.clone();
        for _ in 0..(rnd() % 8 + 1) {
            let i = (rnd() as usize) % m.len();
            m[i] = rnd() as u8;
        }
        if rnd() % 4 == 0 {
            let n = (rnd() as usize) % m.len();
            m.truncate(n);
        }
        let _ = decode_message(&m);
        let _ = unmarshal_body("a{sv}(ddd)asv", m.get(16..).unwrap_or(&[]));
    }
}

#[test]
fn address_parsing() {
    assert_eq!(
        parse_address("unix:path=/run/user/1000/bus").unwrap(),
        BusAddress::Path("/run/user/1000/bus".into())
    );
    assert_eq!(
        parse_address("unix:path=/tmp/a%20b,guid=0123").unwrap(),
        BusAddress::Path("/tmp/a b".into())
    );
    assert_eq!(
        parse_address("unix:abstract=/tmp/dbus-XYZ,guid=1").unwrap(),
        BusAddress::Abstract(b"/tmp/dbus-XYZ".to_vec())
    );
    assert_eq!(
        parse_address("tcp:host=localhost,port=1;unix:path=/x").unwrap(),
        BusAddress::Path("/x".into())
    );
    for bad in [
        "",
        "tcp:host=localhost,port=1",
        "unix:tmpdir=/tmp",
        "unix:path=%zz",
    ] {
        assert!(
            matches!(parse_address(bad), Err(Error::UnsupportedAddress(_))),
            "{bad}"
        );
    }
}

#[test]
fn sasl_external_handshake() {
    let (mut conn, mut server) = pair();
    let t = thread::spawn(move || {
        let mut got = Vec::new();
        let mut b = [0u8; 1];
        while !got.ends_with(b"\r\n") {
            server.read_exact(&mut b).unwrap();
            got.push(b[0]);
        }
        server
            .write_all(b"OK 1234567890abcdef1234567890abcdef\r\n")
            .unwrap();
        let mut begin = [0u8; 7];
        server.read_exact(&mut begin).unwrap();
        (got, begin)
    });
    conn.authenticate(1000).unwrap();
    let (got, begin) = t.join().unwrap();
    assert_eq!(got, b"\0AUTH EXTERNAL 31303030\r\n");
    assert_eq!(&begin, b"BEGIN\r\n");
}

#[test]
fn sasl_rejection_is_typed() {
    let (mut conn, mut server) = pair();
    let t = thread::spawn(move || {
        let mut buf = [0u8; 64];
        let _ = server.read(&mut buf).unwrap();
        server.write_all(b"REJECTED EXTERNAL\r\n").unwrap();
        server
    });
    assert!(matches!(conn.authenticate(1000), Err(Error::Auth(_))));
    drop(t.join());
}

#[test]
fn call_skips_unrelated_messages() {
    let (mut conn, mut server) = pair();
    let t = thread::spawn(move || {
        let (h, body) = server_read(&mut server);
        assert_eq!(h.member.as_deref(), Some("GetCapabilities"));
        assert!(body.is_empty());
        server.write_all(&signal(1)).unwrap();
        server
            .write_all(&reply(2, h.serial + 100, "u", &[Value::Uint32(9)]))
            .unwrap();
        server
            .write_all(&reply(3, h.serial, "as", &[Value::string_array(["body"])]))
            .unwrap();
        server
    });
    let out = conn
        .call("d.e", "/p", "i.f", "GetCapabilities", "", &[])
        .unwrap();
    assert_eq!(out, vec![Value::string_array(["body"])]);
    drop(t.join());
}

#[test]
fn call_maps_error_reply() {
    let (mut conn, mut server) = pair();
    let t = thread::spawn(move || {
        let (h, _) = server_read(&mut server);
        server
            .write_all(&error(
                1,
                h.serial,
                "org.freedesktop.DBus.Error.UnknownMethod",
                "nope",
            ))
            .unwrap();
        server
    });
    match conn.call("d.e", "/p", "i.f", "M", "u", &[Value::Uint32(1)]) {
        Err(Error::Remote { name, message }) => {
            assert_eq!(name, "org.freedesktop.DBus.Error.UnknownMethod");
            assert_eq!(message.as_deref(), Some("nope"));
        }
        other => panic!("unexpected {other:?}"),
    }
    drop(t.join());
}

#[test]
fn call_times_out_and_stays_usable() {
    let (mut conn, mut server) = pair();
    conn.set_timeout(Duration::from_millis(100));
    let start = Instant::now();
    assert!(matches!(
        conn.call("d.e", "/p", "i.f", "M", "", &[]),
        Err(Error::Timeout)
    ));
    assert!(start.elapsed() < Duration::from_secs(2));
    // No partial read happened, so the next call works; the late reply to
    // the first call is skipped.
    let t = thread::spawn(move || {
        let (h1, _) = server_read(&mut server);
        let (h2, _) = server_read(&mut server);
        server.write_all(&reply(1, h1.serial, "", &[])).unwrap();
        server
            .write_all(&reply(2, h2.serial, "u", &[Value::Uint32(5)]))
            .unwrap();
        server
    });
    conn.set_timeout(Duration::from_secs(2));
    let out = conn.call("d.e", "/p", "i.f", "M", "", &[]).unwrap();
    assert_eq!(out, vec![Value::Uint32(5)]);
    drop(t.join());
}

#[test]
fn partial_reply_then_close_is_disconnected() {
    let (mut conn, mut server) = pair();
    let t = thread::spawn(move || {
        let (h, _) = server_read(&mut server);
        let r = reply(1, h.serial, "s", &[Value::str("hello")]);
        server.write_all(&r[..r.len() - 3]).unwrap();
        drop(server);
    });
    assert!(matches!(
        conn.call("d.e", "/p", "i.f", "M", "", &[]),
        Err(Error::Disconnected)
    ));
    t.join().unwrap();
    assert!(matches!(
        conn.call("d.e", "/p", "i.f", "M", "", &[]),
        Err(Error::Disconnected)
    ));
}

#[test]
fn malformed_reply_body_is_typed_error() {
    let (mut conn, mut server) = pair();
    let t = thread::spawn(move || {
        let (h, _) = server_read(&mut server);
        let mut r = reply(1, h.serial, "b", &[Value::Bool(true)]);
        let n = r.len();
        r[n - 4] = 7; // boolean 7
        server.write_all(&r).unwrap();
        server
    });
    assert!(matches!(
        conn.call("d.e", "/p", "i.f", "M", "", &[]),
        Err(Error::Malformed { .. })
    ));
    drop(t.join());
}

#[test]
fn oversized_frame_is_rejected_without_allocation() {
    let (mut conn, mut server) = pair();
    let t = thread::spawn(move || {
        let _ = server_read(&mut server);
        let mut m = [0u8; 16];
        m[..4].copy_from_slice(&[b'l', 2, 0, 1]);
        m[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        server.write_all(&m).unwrap();
        server
    });
    assert!(matches!(
        conn.call("d.e", "/p", "i.f", "M", "", &[]),
        Err(Error::MessageTooLarge(_))
    ));
    drop(t.join());
}

/// A message of `kind` with an `ay` body of `n` zero bytes, built by hand
/// because [`encode_message`] refuses anything over [`MAX_MESSAGE_SIZE`].
fn oversized(kind: MessageKind, serial: u32, reply_to: Option<u32>, n: usize) -> Vec<u8> {
    let mut h = Header::new(kind, serial);
    if kind == MessageKind::Signal {
        h.path = Some("/org/example/Flood".into());
        h.interface = Some("org.example.Flood".into());
        h.member = Some("Big".into());
    }
    h.reply_serial = reply_to;
    h.signature = "ay".into();
    let small = encode_message(
        &h,
        &[Value::Array {
            elem_sig: "y".into(),
            items: vec![],
        }],
    )
    .unwrap();
    let header_len = small.len() - 4; // body was the 4-byte array length
    let mut m = small[..header_len].to_vec();
    m[4..8].copy_from_slice(&(4 + n as u32).to_le_bytes());
    m.extend_from_slice(&(n as u32).to_le_bytes());
    m.resize(m.len() + n, 0);
    m
}

#[test]
fn oversized_unrelated_signal_is_skipped_and_connection_stays_usable() {
    let (mut conn, mut server) = pair();
    conn.set_timeout(Duration::from_secs(5));
    let t = thread::spawn(move || {
        let (h, _) = server_read(&mut server);
        // A same-user peer floods us with a 2 MiB unicast signal first.
        server
            .write_all(&oversized(MessageKind::Signal, 1, None, 2 << 20))
            .unwrap();
        // ...and an oversized reply to some other serial.
        server
            .write_all(&oversized(
                MessageKind::MethodReturn,
                2,
                Some(h.serial + 100),
                (1 << 20) + 1,
            ))
            .unwrap();
        server
            .write_all(&reply(3, h.serial, "u", &[Value::Uint32(7)]))
            .unwrap();
        let (h2, _) = server_read(&mut server);
        server
            .write_all(&reply(4, h2.serial, "u", &[Value::Uint32(8)]))
            .unwrap();
        server
    });
    let out = conn.call("d.e", "/p", "i.f", "M", "", &[]).unwrap();
    assert_eq!(out, vec![Value::Uint32(7)]);
    let out = conn.call("d.e", "/p", "i.f", "M", "", &[]).unwrap();
    assert_eq!(out, vec![Value::Uint32(8)]);
    drop(t.join());
}

#[test]
fn oversized_reply_to_our_serial_is_too_large_and_stream_stays_in_sync() {
    let (mut conn, mut server) = pair();
    conn.set_timeout(Duration::from_secs(5));
    let t = thread::spawn(move || {
        let (h, _) = server_read(&mut server);
        server
            .write_all(&oversized(
                MessageKind::MethodReturn,
                1,
                Some(h.serial),
                2 << 20,
            ))
            .unwrap();
        let (h2, _) = server_read(&mut server);
        server
            .write_all(&reply(2, h2.serial, "u", &[Value::Uint32(9)]))
            .unwrap();
        server
    });
    match conn.call("d.e", "/p", "i.f", "M", "", &[]) {
        Err(Error::MessageTooLarge(n)) => assert!(n > (2 << 20)),
        other => panic!("unexpected {other:?}"),
    }
    // The oversized frame was consumed whole, so the next call works.
    let out = conn.call("d.e", "/p", "i.f", "M", "", &[]).unwrap();
    assert_eq!(out, vec![Value::Uint32(9)]);
    drop(t.join());
}
