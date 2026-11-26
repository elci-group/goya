//! `org.freedesktop.portal.Settings` reader for the
//! `org.freedesktop.appearance` namespace.
//!
//! Implemented: one-shot reads of `color-scheme`, `accent-color` and
//! `contrast`, using `ReadOne` and falling back to the deprecated `Read`
//! when the portal does not know `ReadOne`. Not implemented: the
//! `SettingChanged` signal (no live updates), `ReadAll`, other namespaces.

use crate::dbus::{Connection, Error, Value};

pub const DESTINATION: &str = "org.freedesktop.portal.Desktop";
pub const PATH: &str = "/org/freedesktop/portal/desktop";
pub const INTERFACE: &str = "org.freedesktop.portal.Settings";
pub const NAMESPACE: &str = "org.freedesktop.appearance";

const UNKNOWN_METHOD: &str = "org.freedesktop.DBus.Error.UnknownMethod";

/// `color-scheme` (u32).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorScheme {
    #[default]
    NoPreference,
    PreferDark,
    PreferLight,
    /// A value the specification does not define.
    Unknown(u32),
}

impl From<u32> for ColorScheme {
    fn from(v: u32) -> Self {
        match v {
            0 => ColorScheme::NoPreference,
            1 => ColorScheme::PreferDark,
            2 => ColorScheme::PreferLight,
            n => ColorScheme::Unknown(n),
        }
    }
}

/// `contrast` (u32).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Contrast {
    #[default]
    Normal,
    High,
    /// A value the specification does not define.
    Unknown(u32),
}

impl From<u32> for Contrast {
    fn from(v: u32) -> Self {
        match v {
            0 => Contrast::Normal,
            1 => Contrast::High,
            n => Contrast::Unknown(n),
        }
    }
}

/// How one key's value was obtained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySource {
    /// `ReadOne(ss) -> v`.
    ReadOne,
    /// Deprecated `Read(ss) -> v` (value wrapped in an extra variant).
    Read,
    /// The key could not be read; the field holds its default. The string
    /// is the D-Bus error name or a type-mismatch description.
    Unavailable(String),
}

/// Per-key provenance of an [`Appearance`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppearanceSource {
    pub color_scheme: KeySource,
    pub accent: KeySource,
    pub contrast: KeySource,
}

/// The `org.freedesktop.appearance` settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    /// `NoPreference` when unavailable.
    pub color_scheme: ColorScheme,
    /// sRGB in [0, 1]. `None` when unavailable or when any component is
    /// outside [0, 1] (the specification's "unset" encoding).
    pub accent: Option<[f64; 3]>,
    /// `Normal` when unavailable.
    pub contrast: Contrast,
    pub source: AppearanceSource,
}

fn is_transport_error(e: &Error) -> bool {
    !matches!(e, Error::Remote { .. } | Error::TypeMismatch { .. })
}

/// Read one setting, peeling variant layers. `prefer_read_one` is cleared
/// when the portal reports `ReadOne` as unknown, so later keys go straight
/// to `Read`.
pub fn read_setting(
    conn: &mut Connection,
    namespace: &str,
    key: &str,
    prefer_read_one: &mut bool,
) -> Result<(Value, KeySource), Error> {
    let args = [Value::str(namespace), Value::str(key)];
    if *prefer_read_one {
        match conn.call(DESTINATION, PATH, INTERFACE, "ReadOne", "ss", &args) {
            Ok(r) => return Ok((single_variant(r)?, KeySource::ReadOne)),
            Err(Error::Remote { name, .. }) if name == UNKNOWN_METHOD => {
                *prefer_read_one = false;
            }
            Err(e) => return Err(e),
        }
    }
    let r = conn.call(DESTINATION, PATH, INTERFACE, "Read", "ss", &args)?;
    Ok((single_variant(r)?, KeySource::Read))
}

fn single_variant(r: Vec<Value>) -> Result<Value, Error> {
    match r.as_slice() {
        [v @ Value::Variant(_)] => Ok(v.peel_variants().clone()),
        _ => Err(Error::TypeMismatch {
            expected: "v".into(),
            found: r.iter().map(Value::signature).collect(),
        }),
    }
}

fn mismatch(expected: &str, v: &Value) -> Error {
    Error::TypeMismatch {
        expected: expected.into(),
        found: v.signature(),
    }
}

/// Interpret an `accent-color` value. `Ok(None)` for out-of-range.
pub fn parse_accent(v: &Value) -> Result<Option<[f64; 3]>, Error> {
    let f = v.as_fields().ok_or_else(|| mismatch("(ddd)", v))?;
    let mut rgb = [0.0; 3];
    if f.len() != 3 {
        return Err(mismatch("(ddd)", v));
    }
    for (slot, c) in rgb.iter_mut().zip(f) {
        *slot = c.as_f64().ok_or_else(|| mismatch("(ddd)", v))?;
    }
    Ok(rgb.iter().all(|c| (0.0..=1.0).contains(c)).then_some(rgb))
}

/// Read and parse one appearance key. `Ok((None, Unavailable))` for a
/// D-Bus error or wrong type; `Err` for transport failures.
fn read_key<T>(
    conn: &mut Connection,
    key: &str,
    prefer_read_one: &mut bool,
    parse: impl Fn(&Value) -> Result<T, Error>,
) -> Result<(Option<T>, KeySource), Error> {
    match read_setting(conn, NAMESPACE, key, prefer_read_one)
        .and_then(|(v, src)| Ok((parse(&v)?, src)))
    {
        Ok((t, src)) => Ok((Some(t), src)),
        Err(e) if is_transport_error(&e) => Err(e),
        Err(Error::Remote { name, .. }) => Ok((None, KeySource::Unavailable(name))),
        Err(e) => Ok((None, KeySource::Unavailable(e.to_string()))),
    }
}

fn parse_u32(v: &Value) -> Result<u32, Error> {
    v.as_u32().ok_or_else(|| mismatch("u", v))
}

/// Read `color-scheme`, `accent-color` and `contrast`.
///
/// Each key is read separately: a D-Bus error or wrong type for one key
/// marks only that key [`KeySource::Unavailable`] and leaves the field at
/// its default. Transport failures (socket error, timeout, malformed
/// reply) are returned as `Err`.
pub fn read_appearance(conn: &mut Connection) -> Result<Appearance, Error> {
    let mut prefer_read_one = true;
    let (scheme, cs_src) = read_key(conn, "color-scheme", &mut prefer_read_one, parse_u32)?;
    let (accent, ac_src) = read_key(conn, "accent-color", &mut prefer_read_one, parse_accent)?;
    let (contrast, ct_src) = read_key(conn, "contrast", &mut prefer_read_one, parse_u32)?;
    Ok(Appearance {
        color_scheme: scheme.map(ColorScheme::from).unwrap_or_default(),
        accent: accent.flatten(),
        contrast: contrast.map(Contrast::from).unwrap_or_default(),
        source: AppearanceSource {
            color_scheme: cs_src,
            accent: ac_src,
            contrast: ct_src,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::testutil::*;
    use std::io::Write;

    fn ddd(r: f64, g: f64, b: f64) -> Value {
        Value::Struct(vec![Value::Double(r), Value::Double(g), Value::Double(b)])
    }

    #[test]
    fn enums_from_u32() {
        assert_eq!(ColorScheme::from(1), ColorScheme::PreferDark);
        assert_eq!(ColorScheme::from(2), ColorScheme::PreferLight);
        assert_eq!(ColorScheme::from(7), ColorScheme::Unknown(7));
        assert_eq!(Contrast::from(0), Contrast::Normal);
        assert_eq!(Contrast::from(1), Contrast::High);
        assert_eq!(Contrast::from(3), Contrast::Unknown(3));
    }

    #[test]
    fn accent_range() {
        assert_eq!(
            parse_accent(&ddd(0.388, 0.816, 0.875)).unwrap(),
            Some([0.388, 0.816, 0.875])
        );
        assert_eq!(parse_accent(&ddd(-1.0, 0.5, 0.5)).unwrap(), None);
        assert_eq!(parse_accent(&ddd(0.5, 1.5, 0.5)).unwrap(), None);
        assert_eq!(parse_accent(&ddd(f64::NAN, 0.5, 0.5)).unwrap(), None);
        assert!(parse_accent(&Value::Uint32(1)).is_err());
        assert!(parse_accent(&Value::Struct(vec![Value::Double(1.0)])).is_err());
    }

    /// Fake portal: answer each request with `respond(member, key)`.
    fn serve(
        mut server: std::os::unix::net::UnixStream,
        n: usize,
        respond: impl Fn(&str, &str, u32, u32) -> Vec<u8> + Send + 'static,
    ) -> std::thread::JoinHandle<Vec<String>> {
        std::thread::spawn(move || {
            let mut log = Vec::new();
            for i in 0..n {
                let (h, body) = server_read(&mut server);
                let member = h.member.clone().unwrap();
                let key = body[1].as_str().unwrap().to_owned();
                log.push(format!("{member} {key}"));
                server
                    .write_all(&respond(&member, &key, i as u32 + 1, h.serial))
                    .unwrap();
            }
            log
        })
    }

    #[test]
    fn read_one_path() {
        let (mut conn, server) = pair();
        let t = serve(server, 3, |_, key, s, rs| match key {
            "color-scheme" => reply(s, rs, "v", &[Value::variant(Value::Uint32(1))]),
            "accent-color" => reply(s, rs, "v", &[Value::variant(ddd(0.388, 0.816, 0.875))]),
            _ => reply(s, rs, "v", &[Value::variant(Value::Uint32(0))]),
        });
        let a = read_appearance(&mut conn).unwrap();
        assert_eq!(a.color_scheme, ColorScheme::PreferDark);
        assert_eq!(a.accent, Some([0.388, 0.816, 0.875]));
        assert_eq!(a.contrast, Contrast::Normal);
        assert_eq!(a.source.color_scheme, KeySource::ReadOne);
        assert_eq!(
            t.join().unwrap(),
            [
                "ReadOne color-scheme",
                "ReadOne accent-color",
                "ReadOne contrast"
            ]
        );
    }

    #[test]
    fn falls_back_to_deprecated_read() {
        let (mut conn, server) = pair();
        let t = serve(server, 4, |member, key, s, rs| {
            if member == "ReadOne" {
                return error(s, rs, UNKNOWN_METHOD, "No such method");
            }
            let inner = match key {
                "color-scheme" => Value::Uint32(2),
                "accent-color" => ddd(2.0, 0.0, 0.0),
                _ => Value::Uint32(1),
            };
            // Read returns the value wrapped in an extra variant.
            reply(s, rs, "v", &[Value::variant(Value::variant(inner))])
        });
        let a = read_appearance(&mut conn).unwrap();
        assert_eq!(a.color_scheme, ColorScheme::PreferLight);
        assert_eq!(a.accent, None, "out-of-range accent means unset");
        assert_eq!(a.contrast, Contrast::High);
        assert_eq!(a.source.accent, KeySource::Read);
        assert_eq!(
            t.join().unwrap(),
            [
                "ReadOne color-scheme",
                "Read color-scheme",
                "Read accent-color",
                "Read contrast"
            ]
        );
    }

    #[test]
    fn one_missing_key_does_not_fail_others() {
        let (mut conn, server) = pair();
        let t = serve(server, 3, |_, key, s, rs| match key {
            "color-scheme" => reply(s, rs, "v", &[Value::variant(Value::Uint32(1))]),
            "accent-color" => error(
                s,
                rs,
                "org.freedesktop.portal.Error.NotFound",
                "Requested setting not found",
            ),
            _ => reply(s, rs, "v", &[Value::variant(Value::str("wrong type"))]),
        });
        let a = read_appearance(&mut conn).unwrap();
        assert_eq!(a.color_scheme, ColorScheme::PreferDark);
        assert_eq!(a.accent, None);
        assert_eq!(
            a.source.accent,
            KeySource::Unavailable("org.freedesktop.portal.Error.NotFound".into())
        );
        assert_eq!(a.contrast, Contrast::Normal);
        assert!(matches!(a.source.contrast, KeySource::Unavailable(_)));
        t.join().unwrap();
    }

    #[test]
    fn transport_failure_is_err() {
        let (mut conn, server) = pair();
        drop(server);
        assert!(read_appearance(&mut conn).is_err());
    }
}
