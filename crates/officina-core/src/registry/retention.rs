//! What the registry keeps, and for how long (AD-42, AD-44).

/// How many daily copies are kept: the newest valid ones, so a damaged copy
/// never pushes out a valid one (D-94).
pub const COPIES_KEPT: usize = 2;
