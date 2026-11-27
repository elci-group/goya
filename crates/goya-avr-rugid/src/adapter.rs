//! [`SpatialSource`] adapter over Rugid's headless vector render path.

use crate::BRIDGE_ID;
use goya_avr::ingestion::{snap, ObservationPayload, Observer, SourceError, SpatialSource};
use goya_avr::representation::RepresentationClass;
use rugid::geometry3d::{Point3D, Rotation3D};
use rugid::physics::light::LightSource;
use rugid::physics::optics::Scene;
use rugid::shapes3d::{self, ShapeType};

/// A static Rugid 3D shape exposed as an AVR spatial source.
///
/// The source definition (shape, base size, lighting) is the state subset
/// consumed by the canonical compiler; render-time state (rotation) is
/// derived from the observer, never mutated here.
pub struct RugidShapeSource {
    pub shape: ShapeType,
    /// Model size at `reference_distance`.
    pub base_size: f32,
    /// Observer distance that renders the model at `base_size`.
    pub reference_distance: f32,
    /// Rugid optics scene (materials and lighting state, `L` and `M`).
    pub scene: Scene,
    /// SVG viewport centre used for the observation canvas.
    pub center: (f32, f32),
}

impl RugidShapeSource {
    pub fn new(shape: ShapeType, base_size: f32) -> Self {
        Self {
            shape,
            base_size,
            reference_distance: 2.0,
            scene: default_scene(),
            center: (400.0, 300.0),
        }
    }

    /// Digest of the static model definition (§7.3 source asset digest).
    /// Lights participate; the scene's faces do not (Rugid derives faces
    /// from the shape at render time).
    fn definition_digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(BRIDGE_ID.as_bytes());
        hasher.update(format!("{:?}", self.shape).as_bytes());
        hasher.update(self.base_size.to_le_bytes());
        hasher.update(self.reference_distance.to_le_bytes());
        for light in &self.scene.lights {
            hasher.update(format!("{light:?}").as_bytes());
        }
        hasher.update((self.scene.ambient_light.0 as u32).to_le_bytes());
        hasher.update((self.scene.ambient_light.1 as u32).to_le_bytes());
        hasher.update((self.scene.ambient_light.2 as u32).to_le_bytes());
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        format!("sha256:{hex}")
    }
}

impl SpatialSource for RugidShapeSource {
    fn source_digest(&self) -> String {
        self.definition_digest()
    }

    fn render(&self, observer: &Observer) -> Result<ObservationPayload, SourceError> {
        if observer.distance <= 0.0 {
            return Err(SourceError::UnsupportedObservation);
        }
        // Ingestion contract: render the snapped observer, so a canonical
        // payload is a function of the quantised observation state (§7.3)
        // and exact retrieval reproduces it bit for bit.
        let o = snap(observer);
        // Camera-model decision: observer (azimuth, elevation) ≡ model
        // rotated by (elevation, azimuth). Rugid's camera is fixed at −Z.
        let rotation = Rotation3D::new(o.elevation_deg, o.azimuth_deg, 0.0);
        // Distance-driven zoom: Rugid's projection has a fixed focal
        // length, so distance maps to equivalent model size.
        let size = self.base_size * (self.reference_distance / o.distance);
        if !size.is_finite() || size <= 0.0 {
            return Err(SourceError::UnsupportedObservation);
        }
        let svg = shapes3d::render_shape(
            self.shape,
            self.center.0,
            self.center.1,
            size,
            rotation,
            1.0,
            Point3D::zero(),
            &self.scene,
        );
        if svg.is_empty() {
            return Err(SourceError::RenderFailed("empty render".into()));
        }
        Ok(ObservationPayload {
            representation: RepresentationClass::Vec,
            bytes: svg.into_bytes(),
        })
    }
}

/// A deterministic default optics scene for canonical compilation: one
/// directional light plus the Scene default ambient. Canonical identity
/// folds the lighting state in, so scenes must be constructed
/// deterministically (§7.3).
pub fn default_scene() -> Scene {
    let mut scene = Scene::new();
    // Rugid's convention: light coming from the viewer's upper left.
    scene.add_light(LightSource::directional(
        Point3D::new(-1.0, 1.0, 2.0),
        (255, 255, 255),
        1.0,
    ));
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_avr::canonical::{compile, states_are_reproducible, CompilationBudget};
    use goya_avr::domain::ObservationDomain;
    use goya_avr::ingestion::Observer;
    use goya_avr::retrieval::{resolve, Resolution, UnsupportedReason};
    use goya_avr::sampling::SamplingPolicy;

    fn observer(azimuth: f32, elevation: f32, distance: f32) -> Observer {
        Observer { azimuth_deg: azimuth, elevation_deg: elevation, distance, fov_deg: 60.0 }
    }

    fn budget() -> CompilationBudget {
        CompilationBudget { max_observations: 4096, max_payload_bytes: 64 << 20 }
    }

    #[test]
    fn source_digest_is_stable_and_definition_sensitive() {
        let a = RugidShapeSource::new(ShapeType::Cube, 100.0);
        let b = RugidShapeSource::new(ShapeType::Cube, 100.0);
        let c = RugidShapeSource::new(ShapeType::F1Car, 100.0);
        assert_eq!(a.source_digest(), b.source_digest());
        assert_ne!(a.source_digest(), c.source_digest());
    }

    #[test]
    fn observation_render_is_deterministic_and_view_dependent() {
        let source = RugidShapeSource::new(ShapeType::Cube, 100.0);
        let front = observer(0.0, 0.0, 2.0);
        let side = observer(90.0, 0.0, 2.0);

        let front_a = source.render(&front).expect("front render");
        let front_b = source.render(&front).expect("front render again");
        assert_eq!(front_a, front_b, "same observer, same payload");
        assert_eq!(front_a.representation, RepresentationClass::Vec);

        let side_svg = source.render(&side).expect("side render");
        assert_ne!(front_a, side_svg, "different observer, different observation");
        assert!(side_svg.bytes.starts_with(b"<"));
    }

    #[test]
    fn canonical_compilation_and_exact_retrieval_round_trip() {
        let source = RugidShapeSource::new(ShapeType::Cube, 100.0);
        let policy = SamplingPolicy { initial_samples: 64, distance: 2.0, fov_deg: 60.0 };
        let report = compile(
            &source,
            ObservationDomain::Spherical,
            &policy,
            &budget(),
        )
        .expect("canonical compilation of a Rugid cube");

        assert!(report.complete);
        assert_eq!(report.generated, 64);
        assert!(report.canonical_digest.starts_with("sha256:"));

        // Exact retrieval of every compiled observation, verified against a
        // direct render of the same observer.
        for record in &report.inventory.observations {
            let azimuth = record.state.orientation[0] as f32 / 100.0;
            let elevation = record.state.orientation[1] as f32 / 100.0;
            let distance = record.state.camera[0] as f32 / 256.0;
            let request = observer(azimuth, elevation, distance);
            match resolve(&report, &request) {
                Resolution::Exact(payload) => {
                    let direct = source.render(&request).expect("direct render");
                    assert_eq!(payload.bytes, direct.bytes);
                }
                other => panic!("compiled observation failed to resolve: {other:?}"),
            }
        }
    }

    #[test]
    fn compilation_is_reproducible_across_runs() {
        let source = RugidShapeSource::new(ShapeType::Cube, 100.0);
        let policy = SamplingPolicy { initial_samples: 16, distance: 2.0, fov_deg: 60.0 };
        let a = compile(
            &source,
            ObservationDomain::Orbital,
            &policy,
            &budget(),
        )
        .expect("first compilation");
        let b = compile(
            &source,
            ObservationDomain::Orbital,
            &policy,
            &budget(),
        )
        .expect("second compilation");
        assert!(states_are_reproducible(&a, &b), "canonical identity must hold");
    }

    #[test]
    fn uncompiled_in_domain_request_fails_explicitly() {
        let source = RugidShapeSource::new(ShapeType::Cube, 100.0);
        let policy = SamplingPolicy { initial_samples: 8, distance: 2.0, fov_deg: 60.0 };
        let report = compile(&source, ObservationDomain::Orbital, &policy, &budget())
            .expect("compilation");

        let between = observer(10.0, 0.0, 2.0);
        assert_eq!(
            resolve(&report, &between),
            Resolution::Unsupported(UnsupportedReason::NotCompiled)
        );
        // No fallback: the store has exactly the compiled payloads.
        assert_eq!(report.store.len(), 8);
    }

    #[test]
    fn out_of_domain_request_is_rejected() {
        let source = RugidShapeSource::new(ShapeType::Cube, 100.0);
        let policy = SamplingPolicy { initial_samples: 8, distance: 2.0, fov_deg: 60.0 };
        let report = compile(&source, ObservationDomain::Hemispherical, &policy, &budget())
            .expect("compilation");

        let below = observer(0.0, -30.0, 2.0);
        assert_eq!(
            resolve(&report, &below),
            Resolution::Unsupported(UnsupportedReason::OutsideDomain)
        );
    }

    #[test]
    fn relational_derivation_resolves_real_rugid_observations() {
        // Phase 2: derive a relational inventory from a real Rugid model;
        // anchors resolve Hot, neighbourhood members resolve Cold, and
        // both are bit-identical to a direct render of the same observer.
        use goya_avr::derivation::{DerivationProfile, InventoryProfile};
        use goya_avr::retrieval::{resolve_derived, DerivedResolution};
        use goya_avr::target::{derive, Tier};

        let source = RugidShapeSource::new(ShapeType::Cube, 100.0);
        let policy = SamplingPolicy { initial_samples: 256, distance: 2.0, fov_deg: 60.0 };
        let report = compile(&source, ObservationDomain::Spherical, &policy, &budget())
            .expect("canonical compilation");

        let profile = DerivationProfile {
            profile: InventoryProfile::Relational,
            canonical_digest: report.canonical_digest.clone(),
            index: goya_avr::derivation::IndexPolicy {
                primary_nodes: 16,
                max_neighbourhood_size: 64,
                preserve_canonical_granularity: true,
                allow_pruning: false,
            },
            reconstruction: goya_avr::derivation::ReconstructionPolicy {
                max_dependency_depth: 4,
                allowed_encodings: vec![
                    goya_avr::encoding::EncodingClass::Anchor,
                    goya_avr::encoding::EncodingClass::Copy,
                    goya_avr::encoding::EncodingClass::Shared,
                ],
            },
            residency: goya_avr::derivation::ResidencyPolicy {
                asset_budget_bytes: u64::MAX,
                prefetch_neighbourhoods: 1,
            },
            max_silhouette_error_px: 1,
            allow_live_fallback: false,
        };
        let derivation = derive(&report, &profile).expect("relational derivation");
        assert_eq!(derivation.metrics.canonical_observations, 256);
        assert_eq!(derivation.metrics.directly_indexed, 16);
        assert_eq!(derivation.metrics.retained_observations, 256);
        assert!(derivation.metrics.residency_reduction_bp >= 8000);

        // Every retained observation resolves exactly; a sample of them is
        // bit-identical to a direct render.
        let mut hot_seen = 0;
        let mut cold_seen = 0;
        for record in &derivation.inventory.observations {
            let azimuth = record.state.orientation[0] as f32 / 100.0;
            let elevation = record.state.orientation[1] as f32 / 100.0;
            let distance = record.state.camera[0] as f32 / 256.0;
            let request = observer(azimuth, elevation, distance);
            match resolve_derived(&derivation, &request) {
                DerivedResolution::Exact { payload, tier } => {
                    match tier {
                        Tier::Hot => hot_seen += 1,
                        Tier::Cold => cold_seen += 1,
                    }
                    if cold_seen <= 4 || hot_seen <= 4 {
                        let direct = source.render(&request).expect("direct render");
                        assert_eq!(payload.bytes, direct.bytes);
                    }
                }
                other => panic!("retained observation failed to resolve: {other:?}"),
            }
        }
        assert_eq!(hot_seen, 16, "anchors resolve from the hot tier");
        assert_eq!(cold_seen, 240, "neighbourhood members resolve from the cold tier");

        // In-domain but under-observed states fail explicitly.
        let between = observer(1.0, 1.0, 2.0);
        assert_eq!(
            resolve_derived(&derivation, &between),
            DerivedResolution::Unsupported(goya_avr::retrieval::UnsupportedReason::NotCompiled)
        );
    }

    #[test]
    fn vehicle_shapes_compile_and_retrieve() {
        // §20 POC shapes: the bridge serves them without Rugid changes.
        for shape in [ShapeType::F1Car, ShapeType::MercedesW14] {
            let source = RugidShapeSource::new(shape, 80.0);
            let policy = SamplingPolicy { initial_samples: 8, distance: 2.0, fov_deg: 60.0 };
            let report = compile(&source, ObservationDomain::Orbital, &policy, &budget())
                .unwrap_or_else(|e| panic!("{shape:?} compilation failed: {e:?}"));
            assert!(report.complete);
            let front = observer(0.0, 0.0, 2.0);
            assert!(matches!(resolve(&report, &front), Resolution::Exact(_)));
        }
    }
}
