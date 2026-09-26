//! Features carried by the camiloavelar/herdr fork.
//!
//! Fork code lives here so upstream merges only meet one-line hooks in upstream files.

pub(super) mod agent_groups;
pub(super) mod agent_navigation;
pub(super) mod last_targets;
pub(super) mod navigation_preview;

use agent_navigation::AgentNavigationTarget;
use last_targets::LastTargets;
use navigation_preview::NavigationPreviewOrigin;

/// Fork-owned client shell state, embedded as one field of `ClientShellState`.
#[derive(Debug, Default)]
pub(super) struct ForkShellState {
    pub(super) navigate_agent: Option<AgentNavigationTarget>,
    pub(super) navigation_preview_origin: Option<NavigationPreviewOrigin>,
    pub(super) last_targets: LastTargets,
}
