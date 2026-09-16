use super::*;

/// Client-only agents-panel selection for `NavigateAgents` mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AgentNavigationTarget {
    pub(super) endpoint_id: ClientEndpointId,
    pub(super) pane_id: String,
}

/// Status bar segments shown while `NavigateAgents` mode is active.
pub(super) fn status_bar_segments(
    mode_style: Style,
    base: Style,
    key: Style,
) -> [(String, Style); 6] {
    [
        (" AGENTS ".to_owned(), mode_style),
        (" esc back  ".to_owned(), base),
        ("↑/↓".to_owned(), key),
        (" agent  ".to_owned(), base),
        ("enter".to_owned(), key),
        (" focus".to_owned(), base),
    ]
}

impl ClientShellState {
    fn agent_navigation_targets(&self) -> Vec<AgentNavigationTarget> {
        super::agent_groups::displayed_agent_targets(
            &self.endpoints,
            &self.active_endpoint_id,
            &self.config,
        )
        .into_iter()
        .map(|target| AgentNavigationTarget {
            endpoint_id: target.endpoint_id,
            pane_id: target.pane_id,
        })
        .collect()
    }

    pub(super) fn enter_agent_navigation(&mut self, outcome: &mut ClientShellInput) {
        let targets = self.agent_navigation_targets();
        let focused = self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_pane_id.as_deref());
        self.navigate_agent = targets
            .iter()
            .find(|target| {
                target.endpoint_id == self.active_endpoint_id
                    && Some(target.pane_id.as_str()) == focused
            })
            .or_else(|| targets.first())
            .cloned();
        self.begin_agent_navigation_preview();
        self.mode = ClientShellMode::NavigateAgents;
        outcome.repaint = true;
    }

    fn leave_agent_navigation(&mut self, outcome: &mut ClientShellInput) {
        self.cancel_navigation_preview(outcome);
        self.mode = self.copy_or_terminal_mode();
        self.navigate_agent = None;
    }

    fn move_navigate_agent(&mut self, delta: isize) {
        let targets = self.agent_navigation_targets();
        if targets.is_empty() {
            self.navigate_agent = None;
            return;
        }
        let current = self
            .navigate_agent
            .as_ref()
            .and_then(|selected| targets.iter().position(|target| target == selected));
        let next = match current {
            Some(current) => (current as isize + delta).rem_euclid(targets.len() as isize) as usize,
            None if delta < 0 => targets.len() - 1,
            None => 0,
        };
        let target = &targets[next];
        if self.agent_row_rect(target).is_none() {
            self.agent_scroll = next.min(self.hits.agent_max_scroll);
        }
        self.navigate_agent = Some(target.clone());
    }

    fn accept_navigate_agent(&mut self, outcome: &mut ClientShellInput) {
        let Some(target) = self.navigate_agent.clone() else {
            self.leave_agent_navigation(outcome);
            outcome.repaint = true;
            return;
        };
        if !self.agent_navigation_targets().contains(&target) {
            self.leave_agent_navigation(outcome);
            outcome.repaint = true;
            return;
        }
        if self.focus_or_activate(
            target.endpoint_id,
            ClientEndpointFocusTarget::Pane(target.pane_id),
            outcome,
        ) {
            self.commit_navigation_preview();
            self.mode = ClientShellMode::Terminal;
            self.navigate_agent = None;
        }
        outcome.repaint = true;
    }

    pub(super) fn route_navigate_agents_key(
        &mut self,
        key: &crate::input::TerminalKey,
        outcome: &mut ClientShellInput,
    ) {
        if key.code == KeyCode::Esc
            || crate::config::terminal_key_matches_combo(key, self.config.keybinds.prefix)
        {
            self.leave_agent_navigation(outcome);
            outcome.repaint = true;
            return;
        }
        let navigate = &self.config.keybinds.keybinds.navigate;
        if navigate.workspace_up.matches_direct_key(key) {
            self.move_navigate_agent(-1);
            self.preview_navigate_agent(outcome);
            outcome.repaint = true;
            return;
        }
        if navigate.workspace_down.matches_direct_key(key) {
            self.move_navigate_agent(1);
            self.preview_navigate_agent(outcome);
            outcome.repaint = true;
            return;
        }
        let (code, modifiers) = crate::config::normalize_key_combo((key.code, key.modifiers));
        if code == KeyCode::Enter && modifiers.is_empty() {
            self.accept_navigate_agent(outcome);
            return;
        }
        if let Some(binding) =
            crate::input::resolve_prefix_binding(&self.config.keybinds.keybinds, key)
        {
            self.leave_agent_navigation(outcome);
            outcome.repaint = true;
            self.record_binding(binding, outcome);
        }
    }

    /// Rect of the agents-panel row for `target` in the last composed frame, if visible.
    pub(super) fn agent_row_rect(&self, target: &AgentNavigationTarget) -> Option<Rect> {
        self.hits
            .agents
            .iter()
            .filter(|_| target.endpoint_id == self.active_endpoint_id)
            .find(|(_, pane_id)| pane_id == &target.pane_id)
            .map(|(rect, _)| *rect)
            .or_else(|| {
                self.hits
                    .endpoint_agents
                    .iter()
                    .find(|(_, endpoint_id, pane_id)| {
                        endpoint_id == &target.endpoint_id && pane_id == &target.pane_id
                    })
                    .map(|(rect, _, _)| *rect)
            })
    }

    /// Paints navigation overlays after the sidebar has been rendered: the
    /// preview origin row, then the selected agents-panel row.
    pub(super) fn render_navigation_overlays(&self, buffer: &mut Buffer) {
        self.render_navigation_preview_origin(buffer);
        if self.mode != ClientShellMode::NavigateAgents {
            return;
        }
        let Some(rect) = self
            .navigate_agent
            .as_ref()
            .and_then(|target| self.agent_row_rect(target))
        else {
            return;
        };
        let palette = &self.config.palette;
        let background = if palette.selection_bg == ratatui::style::Color::Reset {
            palette.active_row_bg
        } else {
            palette.selection_bg
        };
        buffer.set_style(rect, Style::default().bg(background));
    }
}
