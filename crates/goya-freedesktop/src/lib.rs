//! Desktop-neutral freedesktop.org integration for Goya on Linux.
//!
//! Nothing in this crate is specific to one desktop. COSMIC-specific
//! behaviour lives in `nebula`. The crate uses only `std`.
//!
//! - [`dbus`]: a small blocking session-bus client (SASL `EXTERNAL`,
//!   `Hello`, method calls, bounds-checked marshalling).
//! - [`notifications`]: `org.freedesktop.Notifications` calls and the
//!   platform-neutral [`notifications::Card`] mapping.
//! - [`portal`]: `org.freedesktop.portal.Settings` reads of the
//!   `org.freedesktop.appearance` keys.
//!
//! ```no_run
//! use goya_freedesktop::{dbus::Connection, notifications, portal};
//!
//! let mut bus = Connection::session()?;
//! let caps = notifications::capabilities(&mut bus)?;
//! let card = notifications::Card {
//!     title: "Saved".into(),
//!     body: "3 files written".into(),
//!     ..Default::default()
//! };
//! let id = notifications::notify(&mut bus, &notifications::card_to_notification(&card, &caps))?;
//! notifications::close(&mut bus, id)?;
//! let look = portal::read_appearance(&mut bus)?;
//! println!("{:?}", look.color_scheme);
//! # Ok::<(), goya_freedesktop::dbus::Error>(())
//! ```

pub mod dbus;
pub mod notifications;
pub mod portal;
