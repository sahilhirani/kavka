//! Authenticated cluster connections. Wraps rdkafka clients when the `kafka`
//! feature is enabled (Phase 0); the facade compiles without it so the
//! scaffold builds on a toolchain without CMake.

use crate::profiles::ConnectionProfile;
use crate::{Error, Result};

pub struct ClusterConnection {
    profile: ConnectionProfile,
}

impl ClusterConnection {
    pub async fn connect(profile: ConnectionProfile) -> Result<Self> {
        // Phase 0: build rdkafka AdminClient/Consumer/Producer from the profile
        // (auth matrix per docs/ARCHITECTURE.md D2).
        Ok(Self { profile })
    }

    pub fn profile(&self) -> &ConnectionProfile {
        &self.profile
    }

    /// Every mutating entry point calls this first (D5: read-only is enforced
    /// in core, not the UI).
    pub fn ensure_writable(&self, op: &'static str) -> Result<()> {
        if self.profile.read_only {
            return Err(Error::ReadOnly(op));
        }
        Ok(())
    }
}
