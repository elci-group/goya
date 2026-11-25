//! Native hosts for the platforms Goya serves.
//!
//! Each host selects one [`goya_resolver::Target`] and runs the admitted scene
//! on the shared resolver. `present` projects that scene into the platform's
//! surface record. `opened` stays false: this crate does not open a window,
//! a document, or a device surface, and it does not depend on a compiler.

use goya_resolver::{
    Application, CaretPlace, Diagnostic, Edit, Instance, Phase, PointOutcome, ProjectedNode,
    Resolver, ScrollOutcome, Target,
};

/// The resolved frame labeled for one served platform. `opened` is false.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSurface {
    pub platform: &'static str,
    pub compiler: &'static str,
    pub surface: &'static str,
    pub opened: bool,
    pub revision: u64,
    pub nodes: Vec<ProjectedNode>,
}

/// Android host. Compiler product: Aragon. Surface: `android.view`, closed.
pub struct AndroidHost(Host);
/// Android TV host. Compiler product: Nevada. Surface: `remote`, closed.
pub struct TelevisionHost(Host);
/// Browser host. Compiler product: Maja. Surface: `dom`, closed.
pub struct BrowserHost(Host);
/// iOS host. Compiler product: Neptune. Surface: `uikit`, closed.
pub struct IosHost(Host);
/// Linux host. Compiler product: Emperor. Surface: `wayland`, closed.
pub struct LinuxHost(Host);
/// Windows host. Compiler product: Sash. Surface: `win32`, closed.
pub struct WindowsHost(Host);

macro_rules! native_host {
    ($name:ident, $variant:ident) => {
        impl $name {
            pub const TARGET: Target = Target::$variant;

            pub fn open() -> Result<Self, Diagnostic> {
                Ok(Self(Host::open(Self::TARGET)?))
            }

            pub fn load(&self, source: &str) -> Result<Application, Diagnostic> {
                self.0.load(source)
            }

            pub fn instantiate(&self, application: &Application) -> Result<Instance, Diagnostic> {
                self.0.instantiate(application)
            }

            pub fn present(&self, instance: &mut Instance) -> Result<NativeSurface, Diagnostic> {
                self.0.present(instance)
            }

            pub fn point(
                &self,
                instance: &mut Instance,
                x_mp: i64,
                y_mp: i64,
            ) -> Result<PointOutcome, Diagnostic> {
                self.0.point(instance, x_mp, y_mp)
            }

            pub fn enter(&self, instance: &mut Instance, text: &str) -> Result<String, Diagnostic> {
                self.0.enter(instance, text)
            }

            pub fn edit(
                &self,
                instance: &mut Instance,
                edit: Edit,
            ) -> Result<CaretPlace, Diagnostic> {
                self.0.edit(instance, edit)
            }

            pub fn scroll(
                &self,
                instance: &mut Instance,
                x_mp: i64,
                y_mp: i64,
                dx_mp: i64,
                dy_mp: i64,
            ) -> Result<ScrollOutcome, Diagnostic> {
                self.0.scroll(instance, x_mp, y_mp, dx_mp, dy_mp)
            }
        }
    };
}

native_host!(AndroidHost, Android);
native_host!(TelevisionHost, Television);
native_host!(BrowserHost, Browser);
native_host!(IosHost, Ios);
native_host!(LinuxHost, Linux);
native_host!(WindowsHost, Windows);

struct Host {
    target: Target,
    resolver: Resolver,
}

impl Host {
    fn open(target: Target) -> Result<Self, Diagnostic> {
        Ok(Self {
            target,
            resolver: Resolver::builder().target(target).build()?,
        })
    }

    fn load(&self, source: &str) -> Result<Application, Diagnostic> {
        self.resolver.load(source)
    }

    fn instantiate(&self, application: &Application) -> Result<Instance, Diagnostic> {
        if application.target() != Some(self.target) {
            return Err(Diagnostic::new(
                "GY-RESOLVER-PLATFORM",
                format!("application target does not match {}", self.target.id()),
            ));
        }
        self.resolver.instantiate(application)
    }

    fn present(&self, instance: &mut Instance) -> Result<NativeSurface, Diagnostic> {
        if instance.target() != Some(self.target) {
            return Err(Diagnostic::new(
                "GY-RESOLVER-PLATFORM",
                format!("instance target does not match {}", self.target.id()),
            ));
        }
        match instance.phase() {
            Phase::Initialising | Phase::Running | Phase::Suspended => {}
            phase => {
                return Err(Diagnostic::new(
                    "GY-RESOLVER-PHASE",
                    format!("cannot present while {}", phase.as_str()),
                ));
            }
        }
        Ok(NativeSurface {
            platform: self.target.id(),
            compiler: self.target.compiler(),
            surface: self.target.surface(),
            opened: false,
            revision: instance.revision(),
            nodes: instance.project()?,
        })
    }

    fn point(
        &self,
        instance: &mut Instance,
        x_mp: i64,
        y_mp: i64,
    ) -> Result<PointOutcome, Diagnostic> {
        if instance.target() != Some(self.target) {
            return Err(Diagnostic::new(
                "GY-RESOLVER-PLATFORM",
                format!("instance target does not match {}", self.target.id()),
            ));
        }
        instance.point(x_mp, y_mp)
    }

    fn enter(&self, instance: &mut Instance, text: &str) -> Result<String, Diagnostic> {
        if instance.target() != Some(self.target) {
            return Err(Diagnostic::new(
                "GY-RESOLVER-PLATFORM",
                format!("instance target does not match {}", self.target.id()),
            ));
        }
        instance.enter(text)
    }

    fn edit(&self, instance: &mut Instance, edit: Edit) -> Result<CaretPlace, Diagnostic> {
        if instance.target() != Some(self.target) {
            return Err(Diagnostic::new(
                "GY-RESOLVER-PLATFORM",
                format!("instance target does not match {}", self.target.id()),
            ));
        }
        instance.edit(edit)
    }

    fn scroll(
        &self,
        instance: &mut Instance,
        x_mp: i64,
        y_mp: i64,
        dx_mp: i64,
        dy_mp: i64,
    ) -> Result<ScrollOutcome, Diagnostic> {
        if instance.target() != Some(self.target) {
            return Err(Diagnostic::new(
                "GY-RESOLVER-PLATFORM",
                format!("instance target does not match {}", self.target.id()),
            ));
        }
        instance.scroll(x_mp, y_mp, dx_mp, dy_mp)
    }
}
