//! Reads this machine for the installer: disks (lsblk), previous installs,
//! firmware state. Owned by the UI side. Stub: filled in by the UI agent.

use super::plan::{DiskInfo, Machine, Previous};
use anyhow::Result;

/// All whole disks, with what's on them.
pub fn disks() -> Result<Vec<DiskInfo>> {
    todo!("probe::disks")
}

/// Previous installs made by this flake.
pub fn previous(_disks: &[DiskInfo]) -> Vec<Previous> {
    todo!("probe::previous")
}

pub fn machine() -> Machine {
    todo!("probe::machine")
}
