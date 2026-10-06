use crate::api::schema::{Method, TabTarget, WorkspaceTarget};
use crate::client::shell::*;

/// Client-side focus history behind the `last_workspace` and `last_tab` actions.
#[derive(Debug, Default)]
pub(in crate::client::shell) struct LastTargets {
    /// Space focused before the current one (tmux `switch-client -l`).
    workspace_id: Option<String>,
    /// Per space: the tab focused before its current one (tmux `last-window`).
    tab_by_workspace: HashMap<String, String>,
}

impl ClientShellState {
    /// Records focus changes between the current snapshot and the incoming one.
    /// A server reboot reuses IDs, so its first snapshot is not a focus move.
    pub(in crate::client::shell) fn track_last_targets(
        &mut self,
        next: &ClientShellSnapshot,
        boot_changed: bool,
    ) {
        if boot_changed {
            return;
        }
        let Some(current) = self.snapshot.as_deref() else {
            return;
        };
        if let (Some(previous), Some(focused)) = (
            current.focused_workspace_id.as_ref(),
            next.focused_workspace_id.as_ref(),
        ) {
            if previous != focused {
                self.fork.last_targets.workspace_id = Some(previous.clone());
            }
        }
        if let (Some(previous), Some(focused)) = (
            current.focused_tab_id.as_ref(),
            next.focused_tab_id.as_ref(),
        ) {
            if previous != focused {
                let workspace_of = |snapshot: &ClientShellSnapshot, tab_id: &str| {
                    snapshot
                        .tabs
                        .iter()
                        .find(|tab| tab.tab_id == tab_id)
                        .map(|tab| tab.workspace_id.clone())
                };
                // Only a tab change inside one space counts; switching spaces keeps
                // each space's own history.
                if let (Some(from), Some(to)) =
                    (workspace_of(current, previous), workspace_of(next, focused))
                {
                    if from == to {
                        self.fork
                            .last_targets
                            .tab_by_workspace
                            .insert(from, previous.clone());
                    }
                }
            }
        }
    }

    pub(in crate::client::shell) fn reset_last_targets(&mut self) {
        self.fork.last_targets = LastTargets::default();
    }

    pub(in crate::client::shell) fn last_workspace_method(
        &self,
        snapshot: &ClientShellSnapshot,
    ) -> Option<Method> {
        let workspace_id = self.fork.last_targets.workspace_id.as_ref()?;
        if snapshot.focused_workspace_id.as_ref() == Some(workspace_id)
            || !snapshot
                .workspaces
                .iter()
                .any(|workspace| &workspace.workspace_id == workspace_id)
        {
            return None;
        }
        Some(Method::WorkspaceFocus(WorkspaceTarget {
            workspace_id: workspace_id.clone(),
        }))
    }

    pub(in crate::client::shell) fn last_tab_method(
        &self,
        snapshot: &ClientShellSnapshot,
    ) -> Option<Method> {
        let workspace_id = snapshot.focused_workspace_id.as_ref()?;
        let tab_id = self.fork.last_targets.tab_by_workspace.get(workspace_id)?;
        if snapshot.focused_tab_id.as_ref() == Some(tab_id)
            || !snapshot
                .tabs
                .iter()
                .any(|tab| &tab.tab_id == tab_id && &tab.workspace_id == workspace_id)
        {
            return None;
        }
        Some(Method::TabFocus(TabTarget {
            tab_id: tab_id.clone(),
        }))
    }
}

/// History actions resolve on the client; without a target there is nothing to forward.
pub(in crate::client::shell) fn is_client_only(action: crate::input::KeybindAction) -> bool {
    matches!(
        action,
        crate::input::KeybindAction::LastWorkspace | crate::input::KeybindAction::LastTab
    )
}

impl ClientShellState {
    /// Prefix twice sends the prefix to the pane, unless the user bound the doubled
    /// prefix (for example `last_workspace = "prefix+b"` with prefix `ctrl+b`).
    pub(in crate::client::shell) fn doubled_prefix_passes_through(
        &self,
        key: &crate::input::TerminalKey,
    ) -> bool {
        self.config.keybinds.matches_prefix(key)
            && crate::input::resolve_prefix_binding(&self.config.keybinds.keybinds, key).is_none()
    }
}
