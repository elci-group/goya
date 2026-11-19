//! `org.freedesktop.Notifications` client (Desktop Notifications
//! Specification 1.2) and the platform-neutral [`Card`] mapping.
//!
//! Implemented: `GetServerInformation`, `GetCapabilities`, `Notify` with
//! actions and the `urgency`/`category` hints, `CloseNotification`.
//! Not implemented: the `ActionInvoked`, `NotificationClosed` and
//! `ActivationToken` signals (no signal subscription in [`crate::dbus`]),
//! image hints, sound hints, and other hints.

use crate::dbus::{expect_signature, Connection, Error, Value};

pub const DESTINATION: &str = "org.freedesktop.Notifications";
pub const PATH: &str = "/org/freedesktop/Notifications";
pub const INTERFACE: &str = "org.freedesktop.Notifications";

/// Reply of `GetServerInformation`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub spec_version: String,
}

/// The `urgency` hint (sent as a byte).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

impl Urgency {
    pub fn as_byte(self) -> u8 {
        match self {
            Urgency::Low => 0,
            Urgency::Normal => 1,
            Urgency::Critical => 2,
        }
    }
}

/// The hints this crate sends.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hints {
    pub urgency: Urgency,
    /// e.g. `"im.received"`; see the specification's category list.
    pub category: Option<String>,
}

/// Arguments of `Notify`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub app_name: String,
    /// 0 for a new notification, or the id of one to replace.
    pub replaces_id: u32,
    /// Icon name or `file://` URI; empty for none.
    pub icon: String,
    pub summary: String,
    /// Sent verbatim. If the server advertises `body-markup` it will be
    /// interpreted as markup; [`card_to_notification`] escapes for that.
    pub body: String,
    /// `(action key, label)` pairs. The key `"default"` is the click action.
    pub actions: Vec<(String, String)>,
    pub hints: Hints,
    /// Milliseconds; -1 lets the server decide, 0 never expires.
    pub expire_timeout: i32,
}

/// Call `GetServerInformation`.
pub fn server_information(conn: &mut Connection) -> Result<ServerInfo, Error> {
    let r = conn.call(
        DESTINATION,
        PATH,
        INTERFACE,
        "GetServerInformation",
        "",
        &[],
    )?;
    expect_signature(&r, "ssss")?;
    let s = |i: usize| r[i].as_str().unwrap_or_default().to_owned();
    Ok(ServerInfo {
        name: s(0),
        vendor: s(1),
        version: s(2),
        spec_version: s(3),
    })
}

/// Call `GetCapabilities`.
pub fn capabilities(conn: &mut Connection) -> Result<Vec<String>, Error> {
    let r = conn.call(DESTINATION, PATH, INTERFACE, "GetCapabilities", "", &[])?;
    expect_signature(&r, "as")?;
    Ok(r[0]
        .as_items()
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect())
}

/// Signature of the `Notify` arguments.
pub const NOTIFY_SIGNATURE: &str = "susssasa{sv}i";

/// The `Notify` argument list for `n`.
pub fn notify_args(n: &Notification) -> Vec<Value> {
    let mut actions = Vec::with_capacity(n.actions.len() * 2);
    for (key, label) in &n.actions {
        actions.push(key.clone());
        actions.push(label.clone());
    }
    let mut hints = vec![("urgency".to_owned(), Value::Byte(n.hints.urgency.as_byte()))];
    if let Some(c) = &n.hints.category {
        hints.push(("category".to_owned(), Value::str(c.clone())));
    }
    vec![
        Value::str(n.app_name.clone()),
        Value::Uint32(n.replaces_id),
        Value::str(n.icon.clone()),
        Value::str(n.summary.clone()),
        Value::str(n.body.clone()),
        Value::string_array(actions),
        Value::vardict(hints),
        Value::Int32(n.expire_timeout),
    ]
}

/// Call `Notify`; returns the server-assigned notification id.
pub fn notify(conn: &mut Connection, n: &Notification) -> Result<u32, Error> {
    let r = conn.call(
        DESTINATION,
        PATH,
        INTERFACE,
        "Notify",
        NOTIFY_SIGNATURE,
        &notify_args(n),
    )?;
    expect_signature(&r, "u")?;
    Ok(r[0].as_u32().unwrap_or_default())
}

/// Call `CloseNotification`.
pub fn close(conn: &mut Connection, id: u32) -> Result<(), Error> {
    conn.call(
        DESTINATION,
        PATH,
        INTERFACE,
        "CloseNotification",
        "u",
        &[Value::Uint32(id)],
    )?;
    Ok(())
}

/// A platform-neutral "Goya Card": the content a Goya app wants to surface,
/// before any desktop-specific mapping. `body` is plain text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Card {
    pub title: String,
    pub body: String,
    /// `(action key, label)` pairs.
    pub actions: Vec<(String, String)>,
    pub urgency: Urgency,
}

/// What [`degrade_card`] had to change to fit the server's capabilities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Degradation {
    /// The server lacks `actions`; this many actions were dropped.
    ActionsDropped(usize),
    /// The server lacks `body`; the body text was dropped.
    BodyDropped,
    /// The server advertises `body-markup`; `<`, `>`, `&` were escaped so
    /// the plain-text body is shown literally.
    BodyEscaped,
}

/// Escape `&`, `<`, `>` for the notification body markup subset.
pub fn escape_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn has(caps: &[String], cap: &str) -> bool {
    caps.iter().any(|c| c == cap)
}

/// Map a [`Card`] to a [`Notification`] for a server with capabilities
/// `caps`, and list what was degraded.
///
/// `app_name` is `"Goya"`, `icon` is empty, `expire_timeout` is -1; callers
/// may overwrite these fields.
pub fn degrade_card(card: &Card, caps: &[String]) -> (Notification, Vec<Degradation>) {
    let mut degraded = Vec::new();
    let actions = if has(caps, "actions") {
        card.actions.clone()
    } else {
        if !card.actions.is_empty() {
            degraded.push(Degradation::ActionsDropped(card.actions.len()));
        }
        Vec::new()
    };
    let body = if !has(caps, "body") {
        if !card.body.is_empty() {
            degraded.push(Degradation::BodyDropped);
        }
        String::new()
    } else if has(caps, "body-markup") {
        let escaped = escape_markup(&card.body);
        if escaped != card.body {
            degraded.push(Degradation::BodyEscaped);
        }
        escaped
    } else {
        card.body.clone()
    };
    let n = Notification {
        app_name: "Goya".into(),
        replaces_id: 0,
        icon: String::new(),
        summary: card.title.clone(),
        body,
        actions,
        hints: Hints {
            urgency: card.urgency,
            category: None,
        },
        expire_timeout: -1,
    };
    (n, degraded)
}

/// [`degrade_card`] without the degradation report.
pub fn card_to_notification(card: &Card, caps: &[String]) -> Notification {
    degrade_card(card, caps).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::testutil::*;
    use crate::dbus::{marshal_body, unmarshal_body};
    use std::io::Write;

    fn caps(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn card() -> Card {
        Card {
            title: "Build <done>".into(),
            body: "a < b && c > d".into(),
            actions: vec![
                ("default".into(), "Open".into()),
                ("dismiss".into(), "Dismiss".into()),
            ],
            urgency: Urgency::Critical,
        }
    }

    #[test]
    fn full_capabilities_keep_everything_and_escape_markup() {
        let cosmic = caps(&[
            "body",
            "icon-static",
            "persistence",
            "actions",
            "action-icons",
            "body-markup",
            "body-hyperlinks",
            "sound",
        ]);
        let (n, d) = degrade_card(&card(), &cosmic);
        assert_eq!(
            n.summary, "Build <done>",
            "summary is plain text, not escaped"
        );
        assert_eq!(n.body, "a &lt; b &amp;&amp; c &gt; d");
        assert_eq!(n.actions.len(), 2);
        assert_eq!(n.hints.urgency, Urgency::Critical);
        assert_eq!(d, vec![Degradation::BodyEscaped]);
    }

    #[test]
    fn no_actions_capability_drops_actions() {
        let (n, d) = degrade_card(&card(), &caps(&["body"]));
        assert!(n.actions.is_empty());
        assert_eq!(n.body, "a < b && c > d", "no markup: body verbatim");
        assert_eq!(d, vec![Degradation::ActionsDropped(2)]);
    }

    #[test]
    fn no_body_capability_drops_body() {
        let (n, d) = degrade_card(&card(), &[]);
        assert!(n.body.is_empty());
        assert!(n.actions.is_empty());
        assert_eq!(
            d,
            vec![Degradation::ActionsDropped(2), Degradation::BodyDropped]
        );
    }

    #[test]
    fn plain_card_has_no_degradations() {
        let c = Card {
            title: "t".into(),
            body: "plain".into(),
            ..Card::default()
        };
        let (n, d) = degrade_card(&c, &caps(&["body", "body-markup"]));
        assert!(d.is_empty());
        assert_eq!(n, card_to_notification(&c, &caps(&["body", "body-markup"])));
        assert_eq!(n.expire_timeout, -1);
        assert_eq!(n.hints.urgency, Urgency::Normal);
    }

    #[test]
    fn notify_args_marshal_and_round_trip() {
        let mut n = card_to_notification(&card(), &caps(&["body", "actions"]));
        n.hints.category = Some("transfer.complete".into());
        let args = notify_args(&n);
        let bytes = marshal_body(NOTIFY_SIGNATURE, &args).unwrap();
        let back = unmarshal_body(NOTIFY_SIGNATURE, &bytes).unwrap();
        assert_eq!(back, args);
        assert_eq!(
            back[5],
            Value::string_array(["default", "Open", "dismiss", "Dismiss"])
        );
        assert_eq!(
            back[6].lookup("urgency"),
            Some(&Value::variant(Value::Byte(2)))
        );
        assert_eq!(
            back[6].lookup("category"),
            Some(&Value::variant(Value::str("transfer.complete")))
        );
    }

    #[test]
    fn notify_and_close_against_fake_server() {
        let (mut conn, mut server) = pair();
        let t = std::thread::spawn(move || {
            let (h, body) = server_read(&mut server);
            assert_eq!(h.member.as_deref(), Some("Notify"));
            assert_eq!(h.signature, NOTIFY_SIGNATURE);
            assert_eq!(body[3].as_str(), Some("Build <done>"));
            server
                .write_all(&reply(1, h.serial, "u", &[Value::Uint32(41)]))
                .unwrap();
            let (h, body) = server_read(&mut server);
            assert_eq!(h.member.as_deref(), Some("CloseNotification"));
            assert_eq!(body, vec![Value::Uint32(41)]);
            server.write_all(&reply(2, h.serial, "", &[])).unwrap();
            server
        });
        let id = notify(&mut conn, &card_to_notification(&card(), &[])).unwrap();
        assert_eq!(id, 41);
        close(&mut conn, id).unwrap();
        drop(t.join());
    }

    #[test]
    fn wrong_reply_type_is_typed_error() {
        let (mut conn, mut server) = pair();
        let t = std::thread::spawn(move || {
            let (h, _) = server_read(&mut server);
            server
                .write_all(&reply(1, h.serial, "s", &[Value::str("x")]))
                .unwrap();
            server
        });
        assert!(matches!(
            capabilities(&mut conn),
            Err(Error::TypeMismatch { .. })
        ));
        drop(t.join());
    }
}
