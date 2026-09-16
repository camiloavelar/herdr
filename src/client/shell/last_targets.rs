use super::*;
use crate::api::schema::{Method, TabTarget, WorkspaceTarget};

/// Client-side focus history behind the `last_workspace` and `last_tab` actions.
#[derive(Debug, Default)]
pub(super) struct LastTargets {
    /// Space focused before the current one (tmux `switch-client -l`).
    workspace_id: Option<String>,
    /// Per space: the tab focused before its current one (tmux `last-window`).
    tab_by_workspace: HashMap<String, String>,
}

impl ClientShellState {
    /// Records focus changes between the current snapshot and the incoming one.
    pub(super) fn track_last_targets(&mut self, next: &ClientShellSnapshot) {
        let Some(current) = self.snapshot.as_deref() else {
            return;
        };
        if let (Some(previous), Some(focused)) = (
            current.focused_workspace_id.as_ref(),
            next.focused_workspace_id.as_ref(),
        ) {
            if previous != focused {
                self.last_targets.workspace_id = Some(previous.clone());
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
                        self.last_targets
                            .tab_by_workspace
                            .insert(from, previous.clone());
                    }
                }
            }
        }
    }

    pub(super) fn reset_last_targets(&mut self) {
        self.last_targets = LastTargets::default();
    }

    pub(super) fn last_workspace_method(&self, snapshot: &ClientShellSnapshot) -> Option<Method> {
        let workspace_id = self.last_targets.workspace_id.as_ref()?;
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

    pub(super) fn last_tab_method(&self, snapshot: &ClientShellSnapshot) -> Option<Method> {
        let workspace_id = snapshot.focused_workspace_id.as_ref()?;
        let tab_id = self.last_targets.tab_by_workspace.get(workspace_id)?;
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
