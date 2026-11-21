//! Package the portable web scene and Neptune's WebKit host for an Apple worker.

use std::{
    fs,
    net::Ipv4Addr,
    path::Path,
};

use serde_json::{json, Value};

pub fn generate(
    source: &Path,
    compiler: &Path,
    bundle_id: &str,
    name: &str,
    output: &Path,
) -> Result<Value, String> {
    if !valid_bundle_id(bundle_id) {
        return Err("bundle ID must have dot-separated ASCII letter or digit components, with optional internal hyphens".into());
    }
    if name.trim().is_empty() || name.chars().any(char::is_control) || name.len() > 128 {
        return Err("app name must contain 1 to 128 UTF-8 bytes without control characters".into());
    }
    fs::create_dir(output).map_err(|error| format!("{}: {error}", output.display()))?;
    let result = generate_into(source, compiler, bundle_id, name, output);
    if result.is_err() {
        if let Err(error) = fs::remove_dir_all(output) {
            return Err(format!("{}; partial project at {} could not be cleaned up: {error}", result.unwrap_err(), output.display()));
        }
    }
    result
}

fn generate_into(
    source: &Path,
    compiler: &Path,
    bundle_id: &str,
    name: &str,
    output: &Path,
) -> Result<Value, String> {
    let web = crate::portable_web::compile(source, compiler, &output.join("Web"))?;
    let endpoint = web
        .get("lan_endpoint")
        .ok_or("portable scene report is missing lan_endpoint")?;
    let endpoint = if endpoint.is_null() {
        None
    } else {
        Some(endpoint.as_str().ok_or("invalid portable scene LAN endpoint")?)
    };
    if output.join("Web/lanlink.js").is_file() != endpoint.is_some() {
        return Err("portable scene LAN endpoint and transport script disagree".into());
    }
    let plist = info_plist(bundle_id, name, endpoint)?;
    fs::create_dir(output.join("Sources")).map_err(|error| error.to_string())?;
    fs::create_dir(output.join("NeptuneScene.xcodeproj")).map_err(|error| error.to_string())?;
    for (path, contents) in [
        ("Sources/App.swift", include_str!("../ios/App.swift")),
        ("Sources/Transport.swift", include_str!("../ios/Transport.swift")),
        ("Info.plist", &plist),
        ("NeptuneScene.xcodeproj/project.pbxproj", include_str!("../ios/project.pbxproj")),
        ("build-device.sh", include_str!("../ios/build-device.sh")),
    ] {
        fs::write(output.join(path), contents).map_err(|error| format!("{path}: {error}"))?;
    }
    let report = json!({
        "schema_version": "1",
        "operation": "ios-project",
        "profile": "neptune.ios.web-scene/0.0.1",
        "bundle_identifier": bundle_id,
        "minimum_ios": "17.0",
        "project": "NeptuneScene.xcodeproj",
        "stages": {
            "project_generation": "PASS",
            "apple_build": "NOT_RUN",
            "device_install": "NOT_RUN",
        },
        "portable_web": web,
    });
    let mut bytes = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    fs::write(output.join("neptune-project.json"), bytes).map_err(|error| error.to_string())?;
    Ok(report)
}

fn valid_bundle_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value.contains('.')
        && value.split('.').all(|part| {
            part.starts_with(|c: char| c.is_ascii_alphanumeric())
                && part.ends_with(|c: char| c.is_ascii_alphanumeric())
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

fn info_plist(bundle_id: &str, name: &str, endpoint: Option<&str>) -> Result<String, String> {
    let name = name.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let lan = match endpoint {
        Some(endpoint) => {
            let (host, port) = endpoint
                .strip_prefix("http://")
                .and_then(|value| value.split_once(':'))
                .ok_or("portable scene LAN endpoint must be an HTTP IPv4 origin with a port")?;
            let ip: Ipv4Addr = host.parse().map_err(|_| "invalid LAN endpoint IPv4 address")?;
            let port: u16 = port.parse().map_err(|_| "invalid LAN endpoint port")?;
            if !ip.is_private() || port == 0 || endpoint != format!("http://{ip}:{port}") {
                return Err("portable scene LAN endpoint must be a canonical private IPv4 HTTP origin".into());
            }
            format!(
                "<key>NeptuneLanEndpoint</key><string>{endpoint}</string>\n\
                 <key>NSLocalNetworkUsageDescription</key><string>Connect to the desktop paired with this scene.</string>\n\
                 <key>NSAppTransportSecurity</key><dict>\n\
                 <key>NSExceptionDomains</key><dict><key>{ip}</key><dict>\n\
                 <key>NSExceptionAllowsInsecureHTTPLoads</key><true/>\n\
                 </dict></dict></dict>\n"
            )
        }
        None => String::new(),
    };
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleDisplayName</key><string>{name}</string>
<key>CFBundleExecutable</key><string>$(EXECUTABLE_NAME)</string>
<key>CFBundleIdentifier</key><string>{bundle_id}</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundleName</key><string>$(PRODUCT_NAME)</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.0.1</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSRequiresIPhoneOS</key><true/>
<key>UILaunchScreen</key><dict/>
<key>UISupportedInterfaceOrientations</key><array>
<string>UIInterfaceOrientationPortrait</string>
<string>UIInterfaceOrientationLandscapeLeft</string>
<string>UIInterfaceOrientationLandscapeRight</string>
</array>
{lan}</dict>
</plist>
"#
    ))
}
