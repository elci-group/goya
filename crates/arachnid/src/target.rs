//! Build-target resolution against the webOS engine caprichios.
//!
//! A target is written `<kind>:<value>`:
//!
//! * `model:OLED65C3PUA`, a model number. Resolves to the platform the model
//!   *shipped* with, which is the oldest software it can be running.
//! * `webos:24`, a platform name (`4.0`, `4.5`, `5`, `6`, `22` … `26`).
//! * `reported:9.2.0`, the version the TV reports in Settings or via
//!   `sdkVersion`.
//! * `ua:<user agent>`, a `Web0S` user-agent string. 4.0 and 4.5 share an
//!   engine, so the oldest matching platform is chosen.
//!
//! With no target, Arachnid builds for the floor (webOS TV 4.0), which runs
//! on every supported TV.

use crate::diagnostics::{Code, Diagnostic};
use goya_caprichios::webos_engine_caprichio::{
    self as caps, platform_by_name, platform_by_reported_version, platforms_by_user_agent,
    WebosPlatform,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Target {
    pub platform: WebosPlatform,
    /// How the target was given, for reports.
    pub requested: String,
    /// Per-series caprichio contract when the target named a catalogued model.
    pub series_contract: Option<&'static str>,
    /// Human description of the resolution.
    pub description: String,
}

impl Target {
    pub fn floor() -> Self {
        Self {
            platform: caps::FLOOR,
            requested: "default".into(),
            series_contract: None,
            description: format!("{} (default floor)", caps::FLOOR),
        }
    }
}

pub fn resolve(spec: Option<&str>) -> Result<Target, Diagnostic> {
    let Some(spec) = spec else {
        return Ok(Target::floor());
    };
    let fail = |e: caps::LookupError| {
        Diagnostic::new(Code::TargetUnresolved, format!("target {spec}: {e}"))
    };
    let (kind, value) = spec.split_once(':').ok_or_else(|| {
        Diagnostic::new(
            Code::TargetUnresolved,
            format!("target {spec}: expected model:<number>, webos:<version>, reported:<version> or ua:<agent>"),
        )
    })?;
    let make = |platform: WebosPlatform, series_contract, description: String| Target {
        platform,
        requested: spec.to_string(),
        series_contract,
        description,
    };
    match kind {
        "model" => {
            let m = caps::decode_model(value).map_err(fail)?;
            let series = m.series.map_or_else(
                || {
                    format!(
                        "{} {} (series not catalogued)",
                        m.model_year,
                        m.family.name()
                    )
                },
                |s| format!("{} {}", m.model_year, s.name),
            );
            let ceiling = m.upgrade_ceiling();
            let upgrade = if ceiling > m.shipped {
                format!("; upgradable to {ceiling}")
            } else {
                String::new()
            };
            Ok(make(
                m.shipped,
                m.series.map(|s| s.contract),
                format!(
                    "{value}: {series}, shipped {} (Chromium {}){upgrade}",
                    m.shipped,
                    m.shipped.engine().chromium_major
                ),
            ))
        }
        "webos" => {
            let p = platform_by_name(value).map_err(fail)?;
            Ok(make(
                p,
                None,
                format!("{p} (Chromium {})", p.engine().chromium_major),
            ))
        }
        "reported" => {
            let p = platform_by_reported_version(value).map_err(fail)?;
            Ok(make(
                p,
                None,
                format!(
                    "reported {value} = {p} (Chromium {})",
                    p.engine().chromium_major
                ),
            ))
        }
        "ua" => {
            let ps = platforms_by_user_agent(value).map_err(fail)?;
            let p = ps[0];
            Ok(make(
                p,
                None,
                format!("user agent = {p} (Chromium {})", p.engine().chromium_major),
            ))
        }
        _ => Err(Diagnostic::new(
            Code::TargetUnresolved,
            format!("target {spec}: unknown kind {kind}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_every_kind() {
        assert_eq!(resolve(None).unwrap().platform, WebosPlatform::Tv4_0);
        let t = resolve(Some("model:OLED65C3PUA")).unwrap();
        assert_eq!(t.platform, WebosPlatform::Tv23);
        assert_eq!(t.series_contract, Some("goya.platform.webos.oled_c3.v1"));
        assert!(
            t.description.contains("upgradable to webOS TV 26"),
            "{}",
            t.description
        );
        assert_eq!(
            resolve(Some("webos:24")).unwrap().platform,
            WebosPlatform::Tv24
        );
        assert_eq!(
            resolve(Some("reported:4.10.2")).unwrap().platform,
            WebosPlatform::Tv4_5
        );
        let ua = "ua:Mozilla/5.0 (Web0S; Linux/SmartTV) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/53.0.2785.34 Safari/537.36 WebAppManager";
        assert_eq!(resolve(Some(ua)).unwrap().platform, WebosPlatform::Tv4_0);
    }

    #[test]
    fn refusals_carry_code_602() {
        for spec in [
            "OLED65C3PUA",
            "model:OLED65B7V",
            "webos:3.5",
            "reported:x",
            "tizen:7",
            "model:OLED65C6PUA",
        ] {
            assert_eq!(
                resolve(Some(spec)).unwrap_err().code,
                Code::TargetUnresolved,
                "{spec}"
            );
        }
    }
}
