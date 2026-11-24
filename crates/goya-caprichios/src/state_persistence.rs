//! Platform caprichio contract for durable application state.
//!
//! GSSH/4 `@persistence` is optional (decision D11). A platform with no
//! declared adapter uses its default local store; a declared adapter must be
//! one the platform implements, or the target refuses it. The host adapter
//! restores before first render and atomically flushes the canonical
//! state record from its pre-suspension callback. A missed lifecycle callback
//! is not a durability guarantee; state transitions should also flush before
//! publishing a durable revision (GY-STATE D8).

use std::collections::BTreeMap;

pub const CONTRACT: &str = "goya.state.persistence.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Platform {
    Android,
    Ios,
    Macos,
    Windows,
    Linux,
    Browser,
}

impl Platform {
    /// Default local store, used when `cache_targets` names no adapter for
    /// this platform.
    pub const fn adapter(self) -> &'static str {
        match self {
            Self::Android => "android.preferences.v1",
            Self::Ios => "ios.preferences.v1",
            Self::Macos => "macos.preferences.v1",
            Self::Windows => "windows.local-settings.v1",
            Self::Linux => "linux.xdg-state.v1",
            Self::Browser => "browser.local-storage.v1",
        }
    }

    /// Every adapter an author may name for this platform: its own default
    /// local store first, then its own database (decision D11). Outside the
    /// browser the database is the SQLite the operating system ships.
    pub const fn adapters(self) -> &'static [&'static str] {
        match self {
            Self::Browser => &["browser.local-storage.v1", "browser.indexeddb.v1"],
            Self::Android => &["android.preferences.v1", "android.sqlite.v1"],
            Self::Ios => &["ios.preferences.v1", "ios.sqlite.v1"],
            Self::Macos => &["macos.preferences.v1", "macos.sqlite.v1"],
            Self::Windows => &["windows.local-settings.v1", "windows.sqlite.v1"],
            Self::Linux => &["linux.xdg-state.v1", "linux.sqlite.v1"],
        }
    }

    /// The platform's own database adapter.
    pub const fn database(self) -> &'static str {
        self.adapters()[1]
    }

    /// The `cache_targets` key for this platform.
    pub const fn name(self) -> &'static str {
        platform_name(self)
    }
}

/// The adapter a platform uses, and whether it was chosen by default.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resolution {
    pub adapter: &'static str,
    /// True when the scene named no adapter for this platform (no
    /// `@persistence`, or a target map without this platform). Compilers
    /// warn when this applies to durable state (decision D11).
    pub defaulted: bool,
}

/// Resolve `platform`'s adapter. `declaration` is `None` when the author
/// omitted `@persistence` (default local store).
pub fn resolve(declaration: Option<&Declaration>, platform: Platform) -> Result<Resolution, Error> {
    let default = Resolution {
        adapter: platform.adapter(),
        defaulted: true,
    };
    let Some(declaration) = declaration else {
        return Ok(default);
    };
    let targets = declaration
        .cache_targets
        .as_ref()
        .ok_or(Error::PersistenceDisabled)?;
    let Some(declared) = targets.get(platform_name(platform)) else {
        return Ok(default);
    };
    platform
        .adapters()
        .iter()
        .copied()
        .find(|adapter| adapter == declared)
        .map(|adapter| Resolution {
            adapter,
            defaulted: false,
        })
        .ok_or_else(|| Error::WrongAdapter {
            platform,
            declared: declared.clone(),
        })
}

/// The adapter a platform uses; see [`resolve`].
pub fn resolve_adapter(
    declaration: Option<&Declaration>,
    platform: Platform,
) -> Result<&'static str, Error> {
    resolve(declaration, platform).map(|resolution| resolution.adapter)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Declaration {
    /// `None` is the authored `null` opt-out, not an omitted declaration.
    pub cache_targets: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleEvent {
    /// The host is about to suspend or background the application.
    WillSuspend,
    /// The host resumed; restore must have completed before UI publication.
    DidResume,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    PersistenceDisabled,
    WrongAdapter {
        platform: Platform,
        declared: String,
    },
    MissingLifecycleHook,
    MissingAtomicStore,
    MissingRestoreBeforeRender,
    Store(String),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub lifecycle_hook: bool,
    pub atomic_store: bool,
    pub restore_before_render: bool,
}

/// Host implementation backed by the selected platform caprichio. `flush`
/// must replace the complete scene record atomically; partial key writes are
/// not permitted. `restore` rejects corrupt or incompatible records as a
/// whole instead of partially applying them.
pub trait Adapter {
    fn flush(&mut self, record_key: &str, record: &[u8]) -> Result<(), Error>;
    fn restore(&mut self, record_key: &str) -> Result<Option<Vec<u8>>, Error>;
}

pub fn validate(
    declaration: Option<&Declaration>,
    platform: Platform,
    capabilities: RuntimeCapabilities,
) -> Result<&'static str, Error> {
    let adapter = resolve_adapter(declaration, platform)?;
    if !capabilities.lifecycle_hook {
        return Err(Error::MissingLifecycleHook);
    }
    if !capabilities.atomic_store {
        return Err(Error::MissingAtomicStore);
    }
    if !capabilities.restore_before_render {
        return Err(Error::MissingRestoreBeforeRender);
    }
    Ok(adapter)
}

/// Platform-specific host adapters use this helper for the suspension hook.
/// It intentionally has no lossy fallback: a failed write is returned to the
/// host so the application can report that durable state was not saved.
pub fn observe_lifecycle(
    adapter: &mut impl Adapter,
    event: LifecycleEvent,
    record_key: &str,
    current_record: &[u8],
) -> Result<Option<Vec<u8>>, Error> {
    match event {
        LifecycleEvent::WillSuspend => {
            adapter.flush(record_key, current_record)?;
            Ok(None)
        }
        LifecycleEvent::DidResume => adapter.restore(record_key),
    }
}

const fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Android => "android",
        Platform::Ios => "ios",
        Platform::Macos => "macos",
        Platform::Windows => "windows",
        Platform::Linux => "linux",
        Platform::Browser => "browser",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Declaration {
        Declaration {
            cache_targets: Some(BTreeMap::from([(
                "android".into(),
                Platform::Android.adapter().into(),
            )])),
        }
    }

    #[test]
    fn target_and_lifecycle_capabilities_are_mandatory() {
        let caps = RuntimeCapabilities {
            lifecycle_hook: true,
            atomic_store: true,
            restore_before_render: true,
        };
        assert_eq!(
            validate(Some(&declaration()), Platform::Android, caps),
            Ok("android.preferences.v1")
        );
        // Unlisted platforms and an omitted declaration use the local default.
        assert_eq!(
            validate(Some(&declaration()), Platform::Ios, caps),
            Ok("ios.preferences.v1")
        );
        assert_eq!(
            validate(None, Platform::Browser, caps),
            Ok("browser.local-storage.v1")
        );
        assert_eq!(
            validate(
                Some(&Declaration {
                    cache_targets: None
                }),
                Platform::Android,
                caps
            ),
            Err(Error::PersistenceDisabled)
        );
        assert_eq!(
            validate(
                Some(&declaration()),
                Platform::Android,
                RuntimeCapabilities::default()
            ),
            Err(Error::MissingLifecycleHook)
        );
    }

    #[test]
    fn browser_defaults_to_local_storage_and_admits_a_declared_database() {
        let declared = |adapter: &str| Declaration {
            cache_targets: Some(BTreeMap::from([("browser".into(), adapter.into())])),
        };
        assert_eq!(
            resolve_adapter(Some(&declared("browser.indexeddb.v1")), Platform::Browser),
            Ok("browser.indexeddb.v1")
        );
        assert_eq!(
            resolve_adapter(
                Some(&declared("browser.local-storage.v1")),
                Platform::Browser
            ),
            Ok("browser.local-storage.v1")
        );
        assert_eq!(
            resolve_adapter(Some(&declared("browser.opfs.v1")), Platform::Browser),
            Err(Error::WrongAdapter {
                platform: Platform::Browser,
                declared: "browser.opfs.v1".into()
            })
        );
        // Only an unnamed platform counts as defaulted.
        assert!(resolve(None, Platform::Browser).unwrap().defaulted);
        let named = declared("browser.local-storage.v1");
        assert!(!resolve(Some(&named), Platform::Browser).unwrap().defaulted);
        assert!(resolve(Some(&named), Platform::Ios).unwrap().defaulted);
        // A database named for another platform does not leak across.
        assert_eq!(
            resolve_adapter(Some(&declared("browser.indexeddb.v1")), Platform::Android),
            Ok("android.preferences.v1")
        );
    }

    #[test]
    fn every_platform_has_its_own_default_and_its_own_database() {
        const ALL: [Platform; 6] = [
            Platform::Android,
            Platform::Ios,
            Platform::Macos,
            Platform::Windows,
            Platform::Linux,
            Platform::Browser,
        ];
        for platform in ALL {
            let prefix = format!("{}.", platform.name());
            assert_eq!(platform.adapters().len(), 2);
            assert_eq!(platform.adapters()[0], platform.adapter());
            for adapter in platform.adapters() {
                assert!(adapter.starts_with(&prefix), "{adapter}");
            }
            let declared = Declaration {
                cache_targets: Some(BTreeMap::from([(
                    platform.name().to_owned(),
                    platform.database().to_owned(),
                )])),
            };
            assert_eq!(
                resolve(Some(&declared), platform),
                Ok(Resolution {
                    adapter: platform.database(),
                    defaulted: false
                })
            );
            // Another platform's database is not accepted.
            for other in ALL.into_iter().filter(|other| *other != platform) {
                let borrowed = Declaration {
                    cache_targets: Some(BTreeMap::from([(
                        platform.name().to_owned(),
                        other.database().to_owned(),
                    )])),
                };
                assert!(resolve(Some(&borrowed), platform).is_err());
            }
        }
    }

    #[derive(Default)]
    struct MemoryAdapter(Option<Vec<u8>>);

    impl Adapter for MemoryAdapter {
        fn flush(&mut self, _: &str, record: &[u8]) -> Result<(), Error> {
            self.0 = Some(record.to_vec());
            Ok(())
        }
        fn restore(&mut self, _: &str) -> Result<Option<Vec<u8>>, Error> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn suspension_flushes_whole_record_and_resume_restores_it() {
        let mut adapter = MemoryAdapter::default();
        observe_lifecycle(
            &mut adapter,
            LifecycleEvent::WillSuspend,
            "goya.state.v1/demo",
            b"record",
        )
        .unwrap();
        assert_eq!(
            observe_lifecycle(
                &mut adapter,
                LifecycleEvent::DidResume,
                "goya.state.v1/demo",
                b""
            )
            .unwrap(),
            Some(b"record".to_vec())
        );
    }
}
