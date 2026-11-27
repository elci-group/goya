//! FreeBSD target manifest implementation.

use goya_diablos::capabilities::manifest::TargetManifest;

/// Create a FreeBSD desktop target manifest.
pub fn freebsd_desktop(arch: &str, version: &str) -> TargetManifest {
    TargetManifest::freebsd_desktop(arch, version)
}

/// Create a FreeBSD server target manifest.
pub fn freebsd_server(arch: &str, version: &str) -> TargetManifest {
    let mut m = TargetManifest::freebsd_desktop(arch, version);
    m.target.profile = "server".to_string();
    m.graphics.backend = "software".to_string();
    m.graphics.gvi = false;
    m.distribution.format = "pkg".to_string();
    m
}

/// Create a FreeBSD headless target manifest.
pub fn freebsd_headless(arch: &str, version: &str) -> TargetManifest {
    let mut m = TargetManifest::freebsd_desktop(arch, version);
    m.target.profile = "headless".to_string();
    m.graphics.backend = "software".to_string();
    m.graphics.gvi = false;
    m.distribution.format = "pkg".to_string();
    m
}

/// Create a FreeBSD constrained/embedded target manifest.
pub fn freebsd_constrained(arch: &str, version: &str) -> TargetManifest {
    let mut m = TargetManifest::freebsd_desktop(arch, version);
    m.target.profile = "constrained".to_string();
    m.resources.optimisation = "minimal".to_string();
    m.graphics.backend = "software".to_string();
    m.graphics.gvi = false;
    m.distribution.format = "pkg".to_string();
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freebsd_desktop_manifest() {
        let m = freebsd_desktop("x86_64", "14.2");
        assert_eq!(m.target.variant, "freebsd");
        assert_eq!(m.target.profile, "desktop");
        assert_eq!(m.security.native_sandbox, "capsicum");
    }

    #[test]
    fn freebsd_server_manifest() {
        let m = freebsd_server("x86_64", "14.2");
        assert_eq!(m.target.profile, "server");
        assert_eq!(m.graphics.backend, "software");
    }

    #[test]
    fn freebsd_constrained_manifest() {
        let m = freebsd_constrained("armv7", "14.2");
        assert_eq!(m.target.profile, "constrained");
        assert_eq!(m.resources.optimisation, "minimal");
    }
}