use crate::client::shell::*;

/// One entry of the agents panel list when grouping by space is enabled.
pub(in crate::client::shell) enum AgentPanelItem<R> {
    Header {
        label: String,
        leading_blank: bool,
        indent: u16,
    },
    Agent {
        row: R,
        indent: u16,
    },
}

/// Columns a space header and its agents shift right under a machine header.
const MACHINE_INDENT: u16 = 2;

impl<R> AgentPanelItem<R> {
    pub(in crate::client::shell) fn lines(&self, agent_lines: impl Fn(&R) -> usize) -> usize {
        match self {
            Self::Header { leading_blank, .. } => 1 + usize::from(*leading_blank),
            Self::Agent { row, .. } => agent_lines(row),
        }
    }
}

/// Grouping applies only to the "spaces" ordering; "priority" stays a flat queue.
pub(in crate::client::shell) fn grouping_enabled(config: &ClientShellConfig) -> bool {
    config.agents.group_by_space
        && config.agent_panel_sort == crate::config::AgentPanelSortConfig::Spaces
}

/// Folds agent rows into header + rows groups, in order of first appearance.
/// `group_of` yields a stable key and the header label; rows without one are
/// kept in place without a header. When `machine_of` yields a (key, label),
/// space groups nest under one header per machine, machines in first-appearance
/// order like the spaces panel.
pub(in crate::client::shell) fn group_items<R>(
    rows: Vec<R>,
    config: &ClientShellConfig,
    machine_of: impl Fn(&R) -> Option<(String, String)>,
    group_of: impl Fn(&R) -> Option<(String, String)>,
    rank_of: impl Fn(&R) -> AgentRank,
) -> Vec<AgentPanelItem<R>> {
    if !grouping_enabled(config) {
        return rows
            .into_iter()
            .map(|row| AgentPanelItem::Agent { row, indent: 0 })
            .collect();
    }
    let mut machines: Vec<Option<(String, String)>> = Vec::new();
    for machine in rows.iter().map(&machine_of) {
        if !machines.contains(&machine) {
            machines.push(machine);
        }
    }
    if machines.iter().all(Option::is_none) {
        return space_items(rows, group_of, rank_of, 0);
    }
    let mut buckets: Vec<Vec<R>> = machines.iter().map(|_| Vec::new()).collect();
    for row in rows {
        let machine = machine_of(&row);
        if let Some(index) = machines.iter().position(|m| *m == machine) {
            buckets[index].push(row);
        }
    }
    let mut items = Vec::new();
    for (index, (machine, rows)) in machines.into_iter().zip(buckets).enumerate() {
        let indent = match machine {
            Some((_, label)) => {
                items.push(AgentPanelItem::Header {
                    label,
                    leading_blank: index > 0,
                    indent: 0,
                });
                MACHINE_INDENT
            }
            None => 0,
        };
        items.extend(space_items(rows, &group_of, &rank_of, indent));
    }
    items
}

/// Space groups of one machine (or of the whole panel). Without a machine
/// header, spaces are separated by a blank line; under one they stack tightly.
fn space_items<R>(
    rows: Vec<R>,
    group_of: impl Fn(&R) -> Option<(String, String)>,
    rank_of: impl Fn(&R) -> AgentRank,
    indent: u16,
) -> Vec<AgentPanelItem<R>> {
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
                leading_blank: indent == 0 && index > 0,
                indent,
            });
        }
        items.extend(
            rows.into_iter()
                .map(|row| AgentPanelItem::Agent { row, indent }),
        );
    }
    items
}

/// `render_agent_list` with space headers when grouping is enabled. Rows are
/// grouped by reference, so rendering does not clone them.
#[allow(clippy::too_many_arguments)] // Mirrors render_agent_list plus the three grouping keys.
pub(in crate::client::shell) fn render_grouped_agent_list<T>(
    buffer: &mut Buffer,
    area: Rect,
    rows: &[T],
    empty_message: Option<&str>,
    config: &ClientShellConfig,
    agent_scroll: &mut usize,
    hits: &mut ShellHitMap,
    row_lines: impl Fn(&T) -> usize,
    mut render_row: impl FnMut(&mut Buffer, Rect, &T, &mut ShellHitMap),
    machine_of: impl Fn(&T) -> Option<(String, String)>,
    group_of: impl Fn(&T) -> Option<(String, String)>,
    rank_of: impl Fn(&T) -> AgentRank,
) {
    let items = group_items(
        rows.iter().collect(),
        config,
        |row| machine_of(row),
        |row| group_of(row),
        |row| rank_of(row),
    );
    crate::client::shell::agent_sidebar::render_agent_list(
        buffer,
        area,
        &items,
        empty_message,
        config,
        agent_scroll,
        hits,
        |item| item.lines(|row| row_lines(row)),
        |buffer, rect, item, hits| match item {
            AgentPanelItem::Header {
                label,
                leading_blank,
                indent,
            } => render_group_header(
                buffer,
                indented(rect, *indent),
                label,
                *leading_blank,
                config,
            ),
            AgentPanelItem::Agent { row, indent } => {
                render_row(buffer, indented(rect, *indent), row, hits)
            }
        },
    );
}

fn indented(rect: Rect, indent: u16) -> Rect {
    let indent = indent.min(rect.width);
    Rect::new(
        rect.x.saturating_add(indent),
        rect.y,
        rect.width - indent,
        rect.height,
    )
}

/// Priority-mode ranking: status urgency first, then most recent state change.
pub(in crate::client::shell) type AgentRank = (u8, u64);

pub(in crate::client::shell) fn agent_rank(
    snapshot: &ClientShellSnapshot,
    pane_id: &str,
) -> AgentRank {
    snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)
        .map(|agent| {
            (
                crate::client::shell::status_priority(agent.agent_status),
                agent.state_change_seq,
            )
        })
        .unwrap_or_default()
}

/// Space (key, label) of the agent living in `pane_id`.
pub(in crate::client::shell) fn workspace_group(
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

pub(in crate::client::shell) fn render_group_header(
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
pub(in crate::client::shell) fn merge_lone_icon_rows(
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

/// Grouped rows show the machine and space once, on the group header.
pub(in crate::client::shell) fn row_machine<'a>(
    machine: Option<&'a str>,
    config: &ClientShellConfig,
) -> Option<&'a str> {
    machine.filter(|_| !grouping_enabled(config))
}

pub(in crate::client::shell) fn row_workspace<'a>(
    label: &'a str,
    config: &ClientShellConfig,
) -> &'a str {
    if grouping_enabled(config) {
        ""
    } else {
        label
    }
}

pub(in crate::client::shell) fn finish_rows(
    rows: Vec<Vec<crate::ui::ResolvedToken>>,
    config: &ClientShellConfig,
) -> Vec<Vec<crate::ui::ResolvedToken>> {
    if grouping_enabled(config) {
        merge_lone_icon_rows(rows)
    } else {
        rows
    }
}

/// Machine (key, label) of an agent when several machines are shown, so space
/// groups nest under a machine header; `None` with a single machine.
pub(in crate::client::shell) fn endpoint_machine(
    endpoints: &[ClientShellEndpoint],
    endpoint_id: &ClientEndpointId,
) -> Option<(String, String)> {
    if endpoints.len() < 2 {
        return None;
    }
    let endpoint = endpoints
        .iter()
        .find(|endpoint| &endpoint.endpoint_id == endpoint_id)?;
    Some((format!("{endpoint_id:?}"), endpoint.label.clone()))
}

/// Group key/label for an agent of one machine: its space.
pub(in crate::client::shell) fn endpoint_group(
    endpoints: &[ClientShellEndpoint],
    endpoint_id: &ClientEndpointId,
    pane_id: &str,
) -> Option<(String, String)> {
    let endpoint = endpoints
        .iter()
        .find(|endpoint| &endpoint.endpoint_id == endpoint_id)?;
    let (workspace_id, label) = workspace_group(endpoint.snapshot.as_deref()?, pane_id)?;
    Some((format!("{endpoint_id:?}/{workspace_id}"), label))
}

pub(in crate::client::shell) fn endpoint_rank(
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
            AgentPanelItem::Agent { row, .. } => Some(row),
            AgentPanelItem::Header { .. } => None,
        })
        .collect()
}

/// Pane ids in the order the local agents panel displays them.
pub(in crate::client::shell) fn displayed_agent_pane_ids(
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
) -> Vec<String> {
    let ids = crate::client::shell::agent_sidebar::ordered_agent_pane_ids(
        snapshot,
        config.agent_panel_sort,
    );
    agents_only(group_items(
        ids,
        config,
        |_| None,
        |pane_id| workspace_group(snapshot, pane_id),
        |pane_id| agent_rank(snapshot, pane_id),
    ))
}

/// Online agent targets in the order the agents panel displays them.
pub(in crate::client::shell) fn displayed_agent_targets(
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
) -> Vec<crate::client::shell::aggregate_navigation::AggregateAgentTarget> {
    let targets = crate::client::shell::aggregate_navigation::online_agent_targets(
        endpoints,
        active_endpoint_id,
        config.agent_panel_sort,
    );
    agents_only(group_items(
        targets,
        config,
        |target| endpoint_machine(endpoints, &target.endpoint_id),
        |target| endpoint_group(endpoints, &target.endpoint_id, &target.pane_id),
        |target| endpoint_rank(endpoints, &target.endpoint_id, &target.pane_id),
    ))
}
