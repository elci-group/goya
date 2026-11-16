use maja_frampton::{AdapterErrorCode, PresentationAdapter, UnavailableAdapter};

#[test]
fn all_inputs_fail_without_producing_artifacts() {
    let adapter = UnavailableAdapter;
    let empty = adapter.lower("").unwrap_err();
    let scene = adapter.lower("authored presentation").unwrap_err();
    assert_eq!(empty, scene);
    assert_eq!(scene.code, AdapterErrorCode::BrowserIntegrationUnavailable);
    assert!(scene
        .to_string()
        .contains("browser integration is unavailable"));
}

#[test]
fn accepts_an_integration_owned_type_without_interpreting_it() {
    struct Presentation;
    let result = UnavailableAdapter.lower(&Presentation);
    assert_eq!(
        result.unwrap_err().code,
        AdapterErrorCode::BrowserIntegrationUnavailable
    );
}
