use super::*;

/// What to refocus when navigation preview is cancelled instead of accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NavigationPreviewRestore {
    Workspace(String),
    Pane(String),
}

/// Focus captured when a navigation mode started with `ui.navigation_preview` on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NavigationPreviewOrigin {
    endpoint_id: ClientEndpointId,
    restore: NavigationPreviewRestore,
    /// Set once a preview focus was sent, so cancel knows a restore is needed.
    previewed: bool,
}

impl ClientShellState {
    fn begin_navigation_preview(&mut self, restore: Option<NavigationPreviewRestore>) {
        self.navigation_preview_origin = None;
        if !self.config.navigation_preview {
            return;
        }
        let Some(restore) = restore else {
            return;
        };
        self.navigation_preview_origin = Some(NavigationPreviewOrigin {
            endpoint_id: self.active_endpoint_id.clone(),
            restore,
            previewed: false,
        });
    }

    pub(super) fn begin_workspace_navigation_preview(&mut self) {
        let restore = self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_workspace_id.clone())
            .map(NavigationPreviewRestore::Workspace);
        self.begin_navigation_preview(restore);
    }

    pub(super) fn begin_agent_navigation_preview(&mut self) {
        let restore = self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_pane_id.clone())
            .map(NavigationPreviewRestore::Pane);
        self.begin_navigation_preview(restore);
    }

    pub(super) fn preview_navigate_workspace(&mut self, outcome: &mut ClientShellInput) {
        let Some(target) = self.navigate_workspace_id.clone() else {
            return;
        };
        self.preview_navigation_focus(
            target.endpoint_id,
            ClientEndpointFocusTarget::Workspace(target.workspace_id),
            outcome,
        );
    }

    pub(super) fn preview_navigate_agent(&mut self, outcome: &mut ClientShellInput) {
        let Some(target) = self.navigate_agent.clone() else {
            return;
        };
        self.preview_navigation_focus(
            target.endpoint_id,
            ClientEndpointFocusTarget::Pane(target.pane_id),
            outcome,
        );
    }

    /// Previews only targets on the origin machine; switching machines is never previewed.
    fn preview_navigation_focus(
        &mut self,
        endpoint_id: ClientEndpointId,
        target: ClientEndpointFocusTarget,
        outcome: &mut ClientShellInput,
    ) {
        let Some(origin) = self.navigation_preview_origin.as_mut() else {
            return;
        };
        if endpoint_id != origin.endpoint_id {
            return;
        }
        origin.previewed = true;
        self.focus_or_activate(endpoint_id, target, outcome);
    }

    /// Restores the origin focus if a preview was shown. Called on every non-Enter exit.
    pub(super) fn cancel_navigation_preview(&mut self, outcome: &mut ClientShellInput) {
        let Some(origin) = self.navigation_preview_origin.take() else {
            return;
        };
        if !origin.previewed || origin.endpoint_id != self.active_endpoint_id {
            return;
        }
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let target = match origin.restore {
            NavigationPreviewRestore::Workspace(workspace_id) => {
                if !snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.workspace_id == workspace_id)
                {
                    return;
                }
                ClientEndpointFocusTarget::Workspace(workspace_id)
            }
            NavigationPreviewRestore::Pane(pane_id) => {
                if !snapshot.panes.iter().any(|pane| pane.pane_id == pane_id) {
                    return;
                }
                ClientEndpointFocusTarget::Pane(pane_id)
            }
        };
        self.focus_or_activate(origin.endpoint_id, target, outcome);
    }

    /// Keeps the row that was focused when navigation started painted as focused
    /// while a preview has moved the real focus elsewhere.
    pub(super) fn render_navigation_preview_origin(&self, buffer: &mut Buffer) {
        if !matches!(
            self.mode,
            ClientShellMode::Navigate | ClientShellMode::NavigateAgents
        ) {
            return;
        }
        let Some(origin) = self.navigation_preview_origin.as_ref() else {
            return;
        };
        if !origin.previewed {
            return;
        }
        let rect = match &origin.restore {
            NavigationPreviewRestore::Workspace(workspace_id) => {
                if self
                    .navigate_workspace_id
                    .as_ref()
                    .is_some_and(|target| target.matches(&origin.endpoint_id, workspace_id))
                {
                    return;
                }
                self.hits
                    .workspaces
                    .iter()
                    .find(|hit| {
                        hit.endpoint_id == origin.endpoint_id && &hit.workspace_id == workspace_id
                    })
                    .map(|hit| hit.rect)
            }
            NavigationPreviewRestore::Pane(pane_id) => {
                let target = AgentNavigationTarget {
                    endpoint_id: origin.endpoint_id.clone(),
                    pane_id: pane_id.clone(),
                };
                if self.navigate_agent.as_ref() == Some(&target) {
                    return;
                }
                self.agent_row_rect(&target)
            }
        };
        if let Some(rect) = rect {
            buffer.set_style(rect, Style::default().bg(self.config.palette.active_row_bg));
        }
    }

    /// Enter keeps the previewed focus; forget the origin.
    pub(super) fn commit_navigation_preview(&mut self) {
        self.navigation_preview_origin = None;
    }
}
