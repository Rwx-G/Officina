//! Work orders (FR-3): what the author launches from an approved template,
//! validated before any argument vector exists; the briefing (FR-1) and
//! the Orchestrator view that holds it beside the work order history; the
//! orchestrator's proposals (FR-11, FR-12).

/// The orchestrator's folder under the Officina home (AD-89).
pub const ORCHESTRATOR_DIR: &str = "orchestrator";

/// The orchestrator session's working directory, under
/// [`ORCHESTRATOR_DIR`]: the only project folder `officina mcp` serves in
/// (AD-90).
pub const ORCHESTRATOR_WORKDIR: &str = "workdir";

/// Where the proposal tools write, under [`ORCHESTRATOR_DIR`] (AD-91).
pub const PROPOSALS_DIR: &str = "proposals";

pub mod briefing;
pub mod card;
pub mod orchestrator;
pub mod proposal;
pub mod view;
pub mod work_order;
