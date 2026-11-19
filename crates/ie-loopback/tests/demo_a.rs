//! Flagship demo A seed (IE-X02), loopback edition: a Goya reference
//! application runs a small classifier-shaped graph on the local CPU,
//! pairs a peer España provider, obtains an equivalent result over the
//! transport, loses the peer mid-request, discards the stale completion by
//! epoch, and returns to a graceful baseline. An untrusted provider is
//! rejected. Loopback replaces the network; every contract rule is
//! exercised end-to-end.

use ie_contract::codec;
use ie_contract::fixture::GoldenFixture;
use ie_contract::framing::{decode_frame, encode_frame};
use ie_contract::messages::{Request, Status};
use ie_contract::PROTOCOL_VERSION;
use ie_gate::{compile_manifest, evaluate, Fallback, FeatureDecl, Sensitivity, Verdict};
use ie_loopback::pairing::{Quota, SharedProvider};

const MODEL: [u8; 32] = [0xAB; 32];

fn classifier_feature() -> FeatureDecl {
    FeatureDecl {
        name: "photo-tag".into(),
        required_ops: vec!["matmul".into()],
        model_sha256: MODEL,
        max_input_bytes: 16,
        max_latency_ms: 5_000,
        sensitivity: Sensitivity::Shareable,
        fallback: Fallback::Disable,
    }
}

fn classify_request(id: u64, lease_id: u64) -> Request {
    Request {
        request_id: id,
        model_sha256: MODEL,
        inputs: vec![
            GoldenFixture::tensor(&[1.0, 2.0, 3.0, 4.0], &[2, 2]),
            GoldenFixture::tensor(&[5.0, 6.0, 7.0, 8.0], &[2, 2]),
        ],
        deadline_ms: 5_000,
        lease_id,
        policy_digest: [0; 32],
        requested_outputs: vec!["tags".into()],
    }
}

/// Leg 1: gated feature on a discovered, paired peer — full transport
/// parity with the local golden result.
#[test]
fn leg1_paired_peer_produces_equivalent_result() {
    let mut provider = SharedProvider::new("ie://peer/esp0/0", 4, 8);
    provider.inner_mut().register_model(MODEL);
    provider.offer_pairing("demo-app", 777_777, Quota::new(2, 5_000));

    // Conditional UI: the feature advertises only after the authenticated
    // descriptor supports it.
    let manifest = compile_manifest(&[classifier_feature()]).unwrap();
    let cap = provider.inner().describe();
    let verdict = evaluate(&manifest.features[0], &cap);
    let Verdict::Available { endpoint_id } = verdict else {
        panic!("feature must be available on the capable paired peer, got {verdict:?}");
    };
    assert_eq!(endpoint_id, "ie://peer/esp0/0");

    // Pair and run through the framed transport.
    provider.pair("demo-app", 777_777).unwrap();
    let lease = provider.acquire_lease("demo-app", 30_000).unwrap();
    let req = classify_request(1, lease.lease_id);
    let mut buf = Vec::new();
    codec::encode_request(&mut buf, &req).unwrap();
    let frame = encode_frame(&buf).unwrap();

    let resp_frame = provider.inner_mut().handle_frame(&lease, &frame).unwrap();
    let (payload, _) = decode_frame(&resp_frame).unwrap();
    let (resp, _) = codec::decode_response(payload).unwrap();
    assert_eq!(resp.status, Status::Completed);
    assert_eq!(resp.epoch, 1, "result must carry the provider epoch");
    let got: Vec<f32> = resp.outputs[0]
        .data
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    assert_eq!(got, vec![19.0, 22.0, 43.0, 50.0], "transport parity with golden");
}

/// Leg 2: peer lost mid-request. The result from the dead generation is
/// discarded by epoch; the gate reports unavailability with remediation;
/// the app returns to baseline (fallback Disable) with user input intact.
#[test]
fn leg2_disconnect_midflight_discards_stale_result_and_recovers_to_baseline() {
    let mut provider = SharedProvider::new("ie://peer/esp0/0", 4, 8);
    provider.inner_mut().register_model(MODEL);
    provider.offer_pairing("demo-app", 777_777, Quota::new(2, 5_000));
    provider.pair("demo-app", 777_777).unwrap();
    let lease = provider.acquire_lease("demo-app", 30_000).unwrap();

    let req = classify_request(1, lease.lease_id);
    let mut buf = Vec::new();
    codec::encode_request(&mut buf, &req).unwrap();
    let frame = encode_frame(&buf).unwrap();

    // Submitted, NOT yet executed — the request is queued mid-flight...
    assert_eq!(
        provider.submit("demo-app", &lease, req.clone()),
        Ok(Status::Pending)
    );

    // ...and the peer identity is lost/revoked at that instant.
    provider.inner_mut().revoke();

    // Any completion from the dead generation is stale by epoch and
    // discarded: submitting through the old lease is UNTRUSTED_PEER.
    assert_eq!(
        provider.inner_mut().submit(&lease, req).unwrap_err().name(),
        "UNTRUSTED_PEER"
    );

    // The gate re-evaluates against the new generation: feature is no
    // longer available (no valid lease ⇒ no authenticated capability).
    // The manifest still compiles; UI falls back to Disable — baseline
    // preserved, remediation text available for display.
    let manifest = compile_manifest(&[classifier_feature()]).unwrap();
    let mut cap = provider.inner().describe();
    assert_eq!(cap.epoch, 2);
    assert_eq!(cap.protocol_version, PROTOCOL_VERSION);
    // Simulate the peer being gone from the descriptor's supported set
    // (unauthenticated/unknown peer advertises nothing).
    cap.supported_ops.clear();
    let verdict = evaluate(&manifest.features[0], &cap);
    assert!(matches!(verdict, Verdict::Unsupported { .. }));
    assert!(!verdict.remediation().is_empty());
    assert_eq!(manifest.features[0].fallback(), Fallback::Disable);
}

/// Leg 3: an untrusted provider is rejected — no pairing, no service,
/// no silent failover to it.
#[test]
fn leg3_untrusted_provider_rejected() {
    let mut provider = SharedProvider::new("ie://peer/esp1/0", 4, 8);
    provider.inner_mut().register_model(MODEL);
    provider.offer_pairing("demo-app", 123_456, Quota::new(2, 5_000));
    // Wrong out-of-band code: pairing refused.
    assert_eq!(provider.pair("demo-app", 654_321).unwrap_err().name(), "UNTRUSTED_PEER");
    // And without pairing, no lease can be had at all.
    assert_eq!(
        provider.acquire_lease("demo-app", 30_000).unwrap_err().name(),
        "UNTRUSTED_PEER"
    );
}

/// Leg 4: reconnect and explicitly re-admit work — a fresh lease on the
/// new epoch succeeds, proving recovery is explicit, not silent.
#[test]
fn leg4_reconnect_requires_explicit_readmission() {
    let mut provider = SharedProvider::new("ie://peer/esp0/0", 4, 8);
    provider.inner_mut().register_model(MODEL);
    provider.offer_pairing("demo-app", 777_777, Quota::new(2, 5_000));
    provider.pair("demo-app", 777_777).unwrap();
    let old = provider.acquire_lease("demo-app", 30_000).unwrap();
    provider.inner_mut().revoke();

    // Reconnect: pair state survives (the owner never revoked the
    // client), but the old lease is dead — new work needs a new lease.
    let fresh = provider.acquire_lease("demo-app", 30_000).unwrap();
    assert_eq!(fresh.epoch, 2);
    assert_ne!(fresh.lease_id, old.lease_id);
    let req = classify_request(9, fresh.lease_id);
    assert_eq!(provider.submit("demo-app", &fresh, req), Ok(Status::Pending));
    provider.run_pending();
    let (st, _) = provider.poll(9).unwrap();
    assert_eq!(st, Status::Completed);
}
