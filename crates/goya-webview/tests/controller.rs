use goya_webview::{BrowserConfig, BrowserError, BrowserSession};
use std::path::PathBuf;

#[test]
fn launch_rejects_missing_executable_without_spawning_a_child() {
    let missing = PathBuf::from("/definitely/not/a/chromium/executable");
    match BrowserSession::launch(BrowserConfig::new(missing, 640, 480)) {
        Err(error) => assert!(matches!(error, BrowserError::InvalidExecutable)),
        Ok(mut browser) => {
            browser.close();
            panic!("missing browser executable unexpectedly launched");
        }
    }
}
