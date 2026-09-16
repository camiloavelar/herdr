use super::render::put_text;
use super::*;

pub(super) fn render_collapsed(
    buffer: &mut Buffer,
    area: Rect,
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
    hits: &mut ShellHitMap,
) {
    let rows = agent_rows(endpoints, active_endpoint_id, config);
    for (index, row) in rows.into_iter().take(area.height as usize).enumerate() {
        let rect = Rect::new(area.x, area.y + index as u16, area.width, 1);
        if row.agent.focused {
            buffer.set_style(rect, Style::default().bg(config.palette.active_row_bg));
        }
        let initial = row.machine_label.chars().next().unwrap_or('?');
        put_text(
            buffer,
            rect.x,
            rect.y,
            rect.width,
            &format!(
                "{initial}{}",
                status_icon(row.agent.status, config.status_indicators)
            ),
            Style::default()
                .fg(if row.stale {
                    config.palette.overlay0
                } else {
                    status_color(row.agent.status, &config.palette)
                })
                .add_modifier(if row.stale {
                    Modifier::DIM
                } else {
                    Modifier::empty()
                }),
        );
        hits.endpoint_agents
            .push((rect, row.endpoint_id, row.agent.pane_id));
    }
}

pub(super) fn render_expanded(
    buffer: &mut Buffer,
    area: Rect,
    agent_view_label: Option<&str>,
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
    agent_scroll: &mut usize,
    hits: &mut ShellHitMap,
) {
    if !super::agent_sidebar::render_agent_panel_header(
        buffer,
        area,
        agent_view_label,
        config,
        hits,
    ) {
        return;
    }
    let rows = agent_rows(endpoints, active_endpoint_id, config);
    let federated = endpoints.len() > 1;
    let items = super::agent_groups::group_items(
        rows,
        config,
        |row| {
            let endpoint = endpoints
                .iter()
                .find(|endpoint| endpoint.endpoint_id == row.endpoint_id)?;
            let (workspace_id, label) = super::agent_groups::workspace_group(
                endpoint.snapshot.as_deref()?,
                &row.agent.pane_id,
            )?;
            let key = format!("{:?}/{workspace_id}", row.endpoint_id);
            let label = if federated {
                format!("{} · {label}", row.machine_label)
            } else {
                label
            };
            Some((key, label))
        },
        |row| {
            endpoints
                .iter()
                .find(|endpoint| endpoint.endpoint_id == row.endpoint_id)
                .and_then(|endpoint| endpoint.snapshot.as_deref())
                .map(|snapshot| super::agent_groups::agent_rank(snapshot, &row.agent.pane_id))
                .unwrap_or_default()
        },
    );
    super::agent_sidebar::render_agent_list(
        buffer,
        area,
        &items,
        agent_view_label.map(|_| " no matching agents"),
        config,
        agent_scroll,
        hits,
        |item| item.lines(|row| row.agent.rows.len()),
        |buffer, rect, item, hits| match item {
            super::agent_groups::AgentPanelItem::Header {
                label,
                leading_blank,
            } => super::agent_groups::render_group_header(
                buffer,
                rect,
                label,
                *leading_blank,
                config,
            ),
            super::agent_groups::AgentPanelItem::Agent(row) => {
                super::agent_sidebar::render_agent_row(buffer, rect, &row.agent, config);
                if row.stale {
                    buffer.set_style(
                        rect,
                        Style::default()
                            .fg(config.palette.overlay0)
                            .add_modifier(Modifier::DIM),
                    );
                }
                hits.endpoint_agents.push((
                    rect,
                    row.endpoint_id.clone(),
                    row.agent.pane_id.clone(),
                ));
            }
        },
    );
}

struct EndpointAgentRow {
    endpoint_id: ClientEndpointId,
    machine_label: String,
    stale: bool,
    agent: super::agent_sidebar::AgentRow,
}

fn agent_rows(
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
) -> Vec<EndpointAgentRow> {
    let mut rendered_rows = endpoints
        .iter()
        .filter_map(|endpoint| {
            endpoint.snapshot.as_deref().map(|snapshot| {
                snapshot
                    .agents
                    .iter()
                    .filter_map(|agent| {
                        super::agent_sidebar::agent_row(
                            snapshot,
                            &agent.pane_id,
                            config,
                            Some(&endpoint.label),
                        )
                    })
                    .map(|agent| ((endpoint.endpoint_id.clone(), agent.pane_id.clone()), agent))
                    .collect::<Vec<_>>()
            })
        })
        .flatten()
        .collect::<HashMap<_, _>>();

    super::aggregate_navigation::aggregate_agent_rows(
        endpoints,
        active_endpoint_id,
        config.agent_panel_sort,
    )
    .into_iter()
    .filter_map(|row| {
        let key = (row.endpoint.endpoint_id.clone(), row.agent.pane_id.clone());
        let mut agent = rendered_rows.remove(&key)?;
        agent.focused &= row.endpoint.endpoint_id == active_endpoint_id;
        Some(EndpointAgentRow {
            endpoint_id: row.endpoint.endpoint_id.clone(),
            machine_label: row.endpoint.label.to_owned(),
            stale: row.endpoint.stale(),
            agent,
        })
    })
    .collect()
}
