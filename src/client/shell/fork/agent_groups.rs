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

/// Columns a nested header and its agents shift right under their parent header.
const NEST_INDENT: u16 = 2;

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

/// Folds agent rows into a tree of headers and rows. `group_of` yields the
/// header path of a row (project, then space), each segment a stable key and
/// its label; a row with an empty path stays without a header. When
/// `machine_of` yields a (key, label), everything nests under one header per
/// machine.
pub(in crate::client::shell) fn group_items<R>(
    rows: Vec<R>,
    config: &ClientShellConfig,
    machine_of: impl Fn(&R) -> Option<(String, String)>,
    group_of: impl Fn(&R) -> Vec<(String, String)>,
    rank_of: impl Fn(&R) -> AgentRank,
) -> Vec<AgentPanelItem<R>> {
    if !grouping_enabled(config) {
        return rows
            .into_iter()
            .map(|row| AgentPanelItem::Agent { row, indent: 0 })
            .collect();
    }
    let by_machine = rows.iter().any(|row| machine_of(row).is_some());
    let rows = rows
        .into_iter()
        .map(|row| {
            let mut path: Vec<_> = machine_of(&row).into_iter().collect();
            path.extend(group_of(&row));
            (path, row)
        })
        .collect();
    let mut items = Vec::new();
    nest(rows, 0, None, usize::from(by_machine), &rank_of, &mut items);
    items
}

type GroupPath = Vec<(String, String)>;
/// Key, label and rows of one header group.
type Group<R> = (String, String, Vec<(GroupPath, R)>);

/// Emits the rows whose path ends at `depth`, then one header per next path
/// segment with its rows nested below. `indent` is the enclosing header's
/// column, `None` at the top. Levels below `ordered_levels` keep
/// first-appearance order (machines, like the spaces panel); deeper levels and
/// rows rank like the priority ordering, a group by its most urgent agent.
/// Top-level groups are separated by a blank line.
fn nest<R>(
    rows: Vec<(GroupPath, R)>,
    depth: usize,
    indent: Option<u16>,
    ordered_levels: usize,
    rank_of: &impl Fn(&R) -> AgentRank,
    items: &mut Vec<AgentPanelItem<R>>,
) {
    let mut direct = Vec::new();
    let mut groups: Vec<Group<R>> = Vec::new();
    for (path, row) in rows {
        let Some((key, label)) = path.get(depth).cloned() else {
            direct.push(row);
            continue;
        };
        match groups.iter_mut().find(|group| group.0 == key) {
            Some(group) => group.2.push((path, row)),
            None => groups.push((key, label, vec![(path, row)])),
        }
    }
    direct.sort_by_key(|row| std::cmp::Reverse(rank_of(row)));
    let row_indent = indent.unwrap_or(0);
    items.extend(direct.into_iter().map(|row| AgentPanelItem::Agent {
        row,
        indent: row_indent,
    }));
    if depth >= ordered_levels {
        groups.sort_by_key(|group| {
            std::cmp::Reverse(group.2.iter().map(|(_, row)| rank_of(row)).max())
        });
    }
    let header_indent = indent.map_or(0, |indent| indent + NEST_INDENT);
    for (index, (_, label, rows)) in groups.into_iter().enumerate() {
        items.push(AgentPanelItem::Header {
            label,
            leading_blank: indent.is_none() && index > 0,
            indent: header_indent,
        });
        nest(
            rows,
            depth + 1,
            Some(header_indent),
            ordered_levels,
            rank_of,
            items,
        );
    }
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
    group_of: impl Fn(&T) -> Vec<(String, String)>,
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

/// Header path of the agent living in `pane_id`: its space, nested under the
/// repository header when the space is a linked worktree. Agents of the
/// repository's main space sit directly under that header, like the spaces panel.
pub(in crate::client::shell) fn workspace_group(
    snapshot: &ClientShellSnapshot,
    pane_id: &str,
) -> Vec<(String, String)> {
    let Some(workspace) = snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)
        .and_then(|agent| {
            snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == agent.workspace_id)
        })
    else {
        return Vec::new();
    };
    let space = (
        format!("space:{}", workspace.workspace_id),
        workspace.label.clone(),
    );
    match &workspace.worktree {
        Some(worktree) if worktree.is_linked_worktree => vec![
            (format!("repo:{}", worktree.key), worktree.label.clone()),
            space,
        ],
        Some(worktree) => vec![(format!("repo:{}", worktree.key), workspace.label.clone())],
        None => vec![space],
    }
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
    let rows = if config.agents.slim {
        slim_rows(rows)
    } else {
        rows
    };
    if grouping_enabled(config) || config.agents.slim {
        merge_lone_icon_rows(rows)
    } else {
        rows
    }
}

/// Slim rows drop the status text and agent name (the state icon color already
/// carries the status) and the rows those tokens leave empty.
fn slim_rows(rows: Vec<Vec<crate::ui::ResolvedToken>>) -> Vec<Vec<crate::ui::ResolvedToken>> {
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .filter(|token| {
                    !matches!(
                        token.kind,
                        crate::ui::ResolvedTokenKind::StateText(_)
                            | crate::ui::ResolvedTokenKind::Agent(_)
                    )
                })
                .collect::<Vec<_>>()
        })
        .filter(|row| !row.is_empty())
        .collect()
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

/// Header path for an agent of one machine; the machine header, when shown,
/// keeps paths of different machines apart.
pub(in crate::client::shell) fn endpoint_group(
    endpoints: &[ClientShellEndpoint],
    endpoint_id: &ClientEndpointId,
    pane_id: &str,
) -> Vec<(String, String)> {
    endpoints
        .iter()
        .find(|endpoint| &endpoint.endpoint_id == endpoint_id)
        .and_then(|endpoint| endpoint.snapshot.as_deref())
        .map(|snapshot| workspace_group(snapshot, pane_id))
        .unwrap_or_default()
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
