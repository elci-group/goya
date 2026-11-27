//! Security synthesis for Diablos (Capsicum, pledge, unveil, kauth, Tauromaquia).

pub mod tauromaquia;
pub mod privilege;
pub mod capsicum;
pub mod pledge;
pub mod kauth;

pub use tauromaquia::{TauromaquiaPolicy, verify_tauromaquia};
pub use privilege::{derive_privileges, ApplicationPrivileges};
pub use capsicum::{CapsicumSandbox, CapsicumRights};
pub use pledge::{PledgeSandbox, PledgePromises};
pub use kauth::{KauthSandbox, KauthListener};