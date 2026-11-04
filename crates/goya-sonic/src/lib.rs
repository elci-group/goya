//! ARSEM — spatial sonic event modulation.
//!
//! Directive GY-SON-001. Maps UI interaction events onto spatial sound
//! sources, guarded by a geometry-revision contract and a per-source
//! token-bucket rate limiter.

use std::collections::HashMap;
use std::fmt;

/// Contract identifier for the sonic modulation protocol.
pub const SONIC_CONTRACT: &str = "GY-SONIC/1";

/// Identity of the UI object that emitted a sonic event.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectId(pub String);

/// Identifier of the sound asset to play for an event.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SoundId(pub String);

/// Playback urgency. `Critical` outranks `Normal`, which outranks `Low`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SonicPriority {
    Low,
    Normal,
    Critical,
}

/// Kind of interaction that produced a sonic event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SonicEventKind {
    Press,
    Release,
    Move,
    Notify,
}

/// A single interaction translated into a sound request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SonicEvent {
    pub source: ObjectId,
    pub kind: SonicEventKind,
    pub geometry_revision: u64,
    pub timestamp_ns: u64,
    pub sound: SoundId,
    pub priority: SonicPriority,
}

/// A rectangular region in logical points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// Point-in-time view of the scene geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometrySnapshot {
    pub revision: u64,
    pub viewport: Rect,
}

impl GeometrySnapshot {
    /// Center of `node` in logical points, clamped to the viewport.
    pub fn center(&self, node: Rect) -> (f32, f32) {
        let (cx, cy) = node.center();
        let vp = &self.viewport;
        let clamp = |v: f32, lo: f32, hi: f32| v.max(lo).min(hi);
        let lo_x = vp.x;
        let hi_x = vp.x + vp.w;
        let lo_y = vp.y;
        let hi_y = vp.y + vp.h;
        (clamp(cx, lo_x, hi_x), clamp(cy, lo_y, hi_y))
    }
}

/// Where the virtual listener is positioned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListenerState {
    pub viewport_center: (f32, f32),
}

/// Errors produced while resolving a sonic event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SonicError {
    /// The geometry snapshot does not match the event's revision.
    StaleGeometry {
        event_revision: u64,
        snapshot_revision: u64,
    },
    /// The node rect is degenerate (negative width or height).
    InvalidNodeRect,
}

impl fmt::Display for SonicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SonicError::StaleGeometry {
                event_revision,
                snapshot_revision,
            } => write!(
                f,
                "stale geometry: event revision {event_revision} != snapshot revision {snapshot_revision}"
            ),
            SonicError::InvalidNodeRect => write!(f, "node rect has negative extent"),
        }
    }
}

impl std::error::Error for SonicError {}

/// Resolved playback position of a sound source in 3D space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialSource {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Resolver trait: maps an event plus geometry context to a spatial source.
///
/// Implementations MUST reject a geometry snapshot whose revision differs
/// from the event's `geometry_revision` with [`SonicError::StaleGeometry`].
pub trait SonicResolver {
    fn resolve(
        &self,
        event: &SonicEvent,
        geometry: &GeometrySnapshot,
        listener: &ListenerState,
        node_rect: Rect,
    ) -> Result<SpatialSource, SonicError>;
}

/// The reference deterministic resolver.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultResolver;

impl SonicResolver for DefaultResolver {
    fn resolve(
        &self,
        event: &SonicEvent,
        geometry: &GeometrySnapshot,
        listener: &ListenerState,
        node_rect: Rect,
    ) -> Result<SpatialSource, SonicError> {
        if node_rect.w < 0.0 || node_rect.h < 0.0 {
            return Err(SonicError::InvalidNodeRect);
        }
        if geometry.revision != event.geometry_revision {
            return Err(SonicError::StaleGeometry {
                event_revision: event.geometry_revision,
                snapshot_revision: geometry.revision,
            });
        }
        let (nx, ny) = geometry.center(node_rect);
        // Deterministic offsets: relative to the listener, attenuated in depth.
        let dx = nx - listener.viewport_center.0;
        let dy = ny - listener.viewport_center.1;
        let dz = match event.priority {
            SonicPriority::Critical => 0.0,
            SonicPriority::Normal => 0.5,
            SonicPriority::Low => 1.0,
        };
        Ok(SpatialSource {
            x: dx,
            y: dy,
            z: dz,
        })
    }
}

/// Playback policy knobs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SonicPolicy {
    pub master_volume: f32,
    pub spatial_enabled: bool,
    pub mono: bool,
    pub rate_limit_per_sec: u32,
    pub muted: bool,
}

impl Default for SonicPolicy {
    fn default() -> Self {
        Self {
            master_volume: 0.35,
            spatial_enabled: true,
            mono: false,
            rate_limit_per_sec: 30,
            muted: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    last_refill_ns: u64,
}

/// Admission engine enforcing policy: mute gate and per-source token bucket.
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    policy: SonicPolicy,
    buckets: HashMap<ObjectId, Bucket>,
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new(SonicPolicy::default())
    }
}

impl PolicyEngine {
    pub fn new(policy: SonicPolicy) -> Self {
        Self {
            policy,
            buckets: HashMap::new(),
        }
    }

    pub fn policy(&self) -> &SonicPolicy {
        &self.policy
    }

    /// Whether an event is admitted for playback.
    ///
    /// Muted mode admits nothing. Otherwise a per-source token bucket
    /// (`rate_limit_per_sec` tokens, refilled linearly from the event
    /// timestamp) gracefully drops high-frequency floods.
    pub fn admit(&mut self, event: &SonicEvent) -> bool {
        if self.policy.muted || self.policy.rate_limit_per_sec == 0 {
            return false;
        }
        let capacity = f64::from(self.policy.rate_limit_per_sec);
        let rate_per_ns = capacity / 1_000_000_000.0;
        let bucket = self.buckets.entry(event.source.clone()).or_insert(Bucket {
            tokens: capacity,
            last_refill_ns: event.timestamp_ns,
        });
        let elapsed_ns = event.timestamp_ns.saturating_sub(bucket.last_refill_ns);
        bucket.tokens = (bucket.tokens + elapsed_ns as f64 * rate_per_ns).min(capacity);
        bucket.last_refill_ns = event.timestamp_ns;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(revision: u64, ts_ns: u64, priority: SonicPriority) -> SonicEvent {
        SonicEvent {
            source: ObjectId("button-1".to_string()),
            kind: SonicEventKind::Press,
            geometry_revision: revision,
            timestamp_ns: ts_ns,
            sound: SoundId("click".to_string()),
            priority,
        }
    }

    fn geometry(revision: u64) -> GeometrySnapshot {
        GeometrySnapshot {
            revision,
            viewport: Rect {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
        }
    }

    fn listener() -> ListenerState {
        ListenerState {
            viewport_center: (400.0, 300.0),
        }
    }

    #[test]
    fn equal_revision_is_accepted() {
        let resolver = DefaultResolver;
        let rect = Rect {
            x: 100.0,
            y: 100.0,
            w: 200.0,
            h: 80.0,
        };
        let out = resolver.resolve(
            &event(7, 0, SonicPriority::Normal),
            &geometry(7),
            &listener(),
            rect,
        );
        assert!(out.is_ok());
    }

    #[test]
    fn stale_revision_is_rejected() {
        let resolver = DefaultResolver;
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let err = resolver
            .resolve(
                &event(9, 0, SonicPriority::Normal),
                &geometry(8),
                &listener(),
                rect,
            )
            .unwrap_err();
        assert_eq!(
            err,
            SonicError::StaleGeometry {
                event_revision: 9,
                snapshot_revision: 8
            }
        );
    }

    #[test]
    fn rate_limiter_drops_floods() {
        let mut engine = PolicyEngine::default();
        let limit = engine.policy().rate_limit_per_sec as usize;
        let mut admitted = 0;
        // Burst of 100 events at the same instant: no refill time, so the
        // bucket admits exactly its capacity and drops the rest.
        for i in 0..100u64 {
            let ev = event(1, i, SonicPriority::Normal);
            if engine.admit(&ev) {
                admitted += 1;
            }
        }
        assert_eq!(admitted, limit);
        assert!(limit < 100, "flood should be dropped, not all admitted");
    }

    #[test]
    fn rate_limiter_refills_over_time() {
        let mut engine = PolicyEngine::default();
        let ev = event(1, 0, SonicPriority::Normal);
        for _ in 0..30 {
            assert!(engine.admit(&ev), "initial burst should be admitted");
        }
        assert!(!engine.admit(&ev), "bucket exhausted");
        // One second later, at least one token has refilled.
        let later = event(1, 1_000_000_000, SonicPriority::Normal);
        assert!(engine.admit(&later));
    }

    #[test]
    fn muted_admits_nothing() {
        let mut engine = PolicyEngine::new(SonicPolicy {
            muted: true,
            ..SonicPolicy::default()
        });
        for i in 0..5u64 {
            let ev = event(1, i * 1_000_000_000, SonicPriority::Critical);
            assert!(!engine.admit(&ev));
        }
    }

    #[test]
    fn priority_ordering() {
        assert!(SonicPriority::Critical > SonicPriority::Normal);
        assert!(SonicPriority::Normal > SonicPriority::Low);
        assert!(SonicPriority::Low < SonicPriority::Critical);
    }

    #[test]
    fn resolution_is_deterministic() {
        let resolver = DefaultResolver;
        let rect = Rect {
            x: 50.0,
            y: 250.0,
            w: 120.0,
            h: 40.0,
        };
        let ev = event(3, 42, SonicPriority::Normal);
        let a = resolver
            .resolve(&ev, &geometry(3), &listener(), rect)
            .unwrap();
        let b = resolver
            .resolve(&ev, &geometry(3), &listener(), rect)
            .unwrap();
        assert_eq!(a, b);
        let (nx, ny) = geometry(3).center(rect);
        assert_eq!(
            a,
            SpatialSource {
                x: nx - 400.0,
                y: ny - 300.0,
                z: 0.5
            }
        );
    }

    #[test]
    fn invalid_node_rect_is_rejected() {
        let resolver = DefaultResolver;
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            w: -1.0,
            h: 10.0,
        };
        let err = resolver
            .resolve(
                &event(1, 0, SonicPriority::Low),
                &geometry(1),
                &listener(),
                rect,
            )
            .unwrap_err();
        assert_eq!(err, SonicError::InvalidNodeRect);
    }
}
