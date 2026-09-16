use super::*;

/// One entry of the agents panel list when grouping by space is enabled.
pub(super) enum AgentPanelItem<R> {
    Header { label: String, leading_blank: bool },
    Agent(R),
}

impl<R> AgentPanelItem<R> {
    pub(super) fn lines(&self, agent_lines: impl Fn(&R) -> usize) -> usize {
        match self {
            Self::Header { leading_blank, .. } => 1 + usize::from(*leading_blank),
            Self::Agent(row) => agent_lines(row),
        }
    }
}

/// Grouping applies only to the "spaces" ordering; "priority" stays a flat queue.
pub(super) fn grouping_enabled(config: &ClientShellConfig) -> bool {
    config.agents.group_by_space
        && config.agent_panel_sort == crate::config::AgentPanelSortConfig::Spaces
}

/// Folds agent rows into header + rows groups, in order of first appearance.
/// `group_of` yields a stable key and the header label; rows without one are
/// kept in place without a header.
pub(super) fn group_items<R>(
    rows: Vec<R>,
    config: &ClientShellConfig,
    group_of: impl Fn(&R) -> Option<(String, String)>,
) -> Vec<AgentPanelItem<R>> {
    if !grouping_enabled(config) {
        return rows.into_iter().map(AgentPanelItem::Agent).collect();
    }
    let mut groups: Vec<(Option<String>, String, Vec<R>)> = Vec::new();
    for row in rows {
        let (key, label) = match group_of(&row) {
            Some((key, label)) => (Some(key), label),
            None => (None, String::new()),
        };
        match groups
            .iter_mut()
            .find(|group| key.is_some() && group.0 == key)
        {
            Some(group) => group.2.push(row),
            None => groups.push((key, label, vec![row])),
        }
    }
    let mut items = Vec::new();
    for (index, (key, label, rows)) in groups.into_iter().enumerate() {
        if key.is_some() {
            items.push(AgentPanelItem::Header {
                label,
                leading_blank: index > 0,
            });
        }
        items.extend(rows.into_iter().map(AgentPanelItem::Agent));
    }
    items
}

/// Space (key, label) of the agent living in `pane_id`.
pub(super) fn workspace_group(
    snapshot: &ClientShellSnapshot,
    pane_id: &str,
) -> Option<(String, String)> {
    let agent = snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)?;
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == agent.workspace_id)?;
    Some((workspace.workspace_id.clone(), workspace.label.clone()))
}

pub(super) fn render_group_header(
    buffer: &mut Buffer,
    rect: Rect,
    label: &str,
    leading_blank: bool,
    config: &ClientShellConfig,
) {
    let y = rect.y.saturating_add(u16::from(leading_blank));
    if y >= rect.bottom() {
        return;
    }
    let style = Style::default()
        .fg(config.palette.overlay1)
        .add_modifier(Modifier::BOLD);
    for (offset, character) in label.chars().take(rect.width as usize).enumerate() {
        if let Some(cell) = buffer.cell_mut((rect.x.saturating_add(offset as u16), y)) {
            cell.set_symbol(&character.to_string());
            cell.set_style(style);
        }
    }
}
