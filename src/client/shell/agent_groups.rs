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
    rank_of: impl Fn(&R) -> AgentRank,
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
    // Same ranking as the priority ordering, inside each space and across spaces
    // (a space sorts by its most urgent agent). Stable, so ties keep space order.
    for group in &mut groups {
        group.2.sort_by_key(|row| std::cmp::Reverse(rank_of(row)));
    }
    groups.sort_by_key(|group| std::cmp::Reverse(group.2.first().map(&rank_of)));
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

/// Priority-mode ranking: status urgency first, then most recent state change.
pub(super) type AgentRank = (u8, u64);

pub(super) fn agent_rank(snapshot: &ClientShellSnapshot, pane_id: &str) -> AgentRank {
    snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)
        .map(|agent| {
            (
                super::status_priority(agent.agent_status),
                agent.state_change_seq,
            )
        })
        .unwrap_or_default()
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

/// With machine/workspace hidden under a group header, a row left holding only
/// the state icon merges into the following row so the icon still leads a line.
pub(super) fn merge_lone_icon_rows(
    rows: Vec<Vec<crate::ui::ResolvedToken>>,
) -> Vec<Vec<crate::ui::ResolvedToken>> {
    let mut merged = Vec::with_capacity(rows.len());
    let mut carry: Vec<crate::ui::ResolvedToken> = Vec::new();
    for row in rows {
        let only_icons = !row.is_empty()
            && row
                .iter()
                .all(|token| matches!(token.kind, crate::ui::ResolvedTokenKind::StateIcon));
        if only_icons {
            carry.extend(row);
            continue;
        }
        let mut line = std::mem::take(&mut carry);
        line.extend(row);
        merged.push(line);
    }
    if !carry.is_empty() {
        merged.push(carry);
    }
    merged
}

/// Group key/label for an agent of one machine: the space, prefixed with the
/// machine label when several machines are shown.
pub(super) fn endpoint_group(
    endpoints: &[ClientShellEndpoint],
    endpoint_id: &ClientEndpointId,
    pane_id: &str,
    federated: bool,
) -> Option<(String, String)> {
    let endpoint = endpoints
        .iter()
        .find(|endpoint| &endpoint.endpoint_id == endpoint_id)?;
    let (workspace_id, label) = workspace_group(endpoint.snapshot.as_deref()?, pane_id)?;
    let key = format!("{endpoint_id:?}/{workspace_id}");
    let label = if federated {
        format!("{} · {label}", endpoint.label)
    } else {
        label
    };
    Some((key, label))
}

pub(super) fn endpoint_rank(
    endpoints: &[ClientShellEndpoint],
    endpoint_id: &ClientEndpointId,
    pane_id: &str,
) -> AgentRank {
    endpoints
        .iter()
        .find(|endpoint| &endpoint.endpoint_id == endpoint_id)
        .and_then(|endpoint| endpoint.snapshot.as_deref())
        .map(|snapshot| agent_rank(snapshot, pane_id))
        .unwrap_or_default()
}

fn agents_only<R>(items: Vec<AgentPanelItem<R>>) -> Vec<R> {
    items
        .into_iter()
        .filter_map(|item| match item {
            AgentPanelItem::Agent(row) => Some(row),
            AgentPanelItem::Header { .. } => None,
        })
        .collect()
}

/// Pane ids in the order the local agents panel displays them.
pub(super) fn displayed_agent_pane_ids(
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
) -> Vec<String> {
    let ids = super::agent_sidebar::ordered_agent_pane_ids(snapshot, config.agent_panel_sort);
    agents_only(group_items(
        ids,
        config,
        |pane_id| workspace_group(snapshot, pane_id),
        |pane_id| agent_rank(snapshot, pane_id),
    ))
}

/// Online agent targets in the order the agents panel displays them.
pub(super) fn displayed_agent_targets(
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
) -> Vec<super::aggregate_navigation::AggregateAgentTarget> {
    let targets = super::aggregate_navigation::online_agent_targets(
        endpoints,
        active_endpoint_id,
        config.agent_panel_sort,
    );
    let federated = endpoints.len() > 1;
    agents_only(group_items(
        targets,
        config,
        |target| endpoint_group(endpoints, &target.endpoint_id, &target.pane_id, federated),
        |target| endpoint_rank(endpoints, &target.endpoint_id, &target.pane_id),
    ))
}
