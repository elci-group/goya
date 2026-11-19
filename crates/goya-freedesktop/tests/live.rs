//! Live tests against the real session bus. All are `#[ignore]`d.
//!
//! Run: `cargo test -p goya-freedesktop --offline -- --ignored --nocapture`
//!
//! `live_notify_and_close` additionally requires
//! `GOYA_FREEDESKTOP_POST_NOTIFICATION=1`, because it puts a visible
//! notification on the desktop (and closes it within one second).

use goya_freedesktop::dbus::Connection;
use goya_freedesktop::{notifications, portal};
use std::time::{Duration, Instant};

fn connect() -> Connection {
    let c = Connection::session().expect("connect to session bus");
    println!("connected as {}", c.unique_name());
    c
}

#[test]
#[ignore = "needs a session bus with a notification server"]
fn live_get_server_information() {
    let mut c = connect();
    let info = notifications::server_information(&mut c).expect("GetServerInformation");
    println!("GetServerInformation -> {info:?}");
    assert!(!info.name.is_empty());
}

#[test]
#[ignore = "needs a session bus with a notification server"]
fn live_get_capabilities() {
    let mut c = connect();
    let caps = notifications::capabilities(&mut c).expect("GetCapabilities");
    println!("GetCapabilities -> {caps:?}");
}

#[test]
#[ignore = "needs a session bus with xdg-desktop-portal"]
fn live_read_appearance() {
    let mut c = connect();
    let a = portal::read_appearance(&mut c).expect("read_appearance");
    println!("read_appearance -> {a:?}");
}

#[test]
#[ignore = "posts a visible notification; also needs GOYA_FREEDESKTOP_POST_NOTIFICATION=1"]
fn live_notify_and_close() {
    if std::env::var("GOYA_FREEDESKTOP_POST_NOTIFICATION").as_deref() != Ok("1") {
        println!("skipped: set GOYA_FREEDESKTOP_POST_NOTIFICATION=1 to post a notification");
        return;
    }
    let mut c = connect();
    let caps = notifications::capabilities(&mut c).expect("GetCapabilities");
    let card = notifications::Card {
        title: "Goya freedesktop live test".into(),
        body: "Closes within one second. <tag> & text".into(),
        actions: vec![("default".into(), "Open".into())],
        urgency: notifications::Urgency::Low,
    };
    let (mut n, degraded) = notifications::degrade_card(&card, &caps);
    n.expire_timeout = 1000;
    let start = Instant::now();
    let id = notifications::notify(&mut c, &n).expect("Notify");
    std::thread::sleep(Duration::from_millis(200));
    notifications::close(&mut c, id).expect("CloseNotification");
    let elapsed = start.elapsed();
    println!(
        "Notify -> id {id}; body sent {:?}; degradations {degraded:?}; closed after {elapsed:?}",
        n.body
    );
    assert!(id != 0);
    assert!(elapsed < Duration::from_secs(1));
}
