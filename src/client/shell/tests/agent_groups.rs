use super::*;

fn agent(pane_id: &str, workspace_id: &str, tab_id: &str) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: workspace_id.into(),
        tab_id: tab_id.into(),
        name: Some(pane_id.into()),
        display_agent: None,
        agent: None,
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: AgentStatus::Idle,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: pane_id == "pane_1",
    }
}

/// ws_1 "client-shell" with pane_1 and ws_2 "second" with pane_2.
fn two_spaces() -> ClientShellSnapshot {
    let mut snap = snapshot();
    let mut ws_2 = snap.workspaces[0].clone();
    ws_2.workspace_id = "ws_2".into();
    ws_2.number = 2;
    ws_2.label = "second".into();
    ws_2.focused = false;
    ws_2.active_tab_id = "tab_2".into();
    snap.workspaces.push(ws_2);
    let mut tab_2 = snap.tabs[0].clone();
    tab_2.tab_id = "tab_2".into();
    tab_2.workspace_id = "ws_2".into();
    tab_2.focused = false;
    snap.tabs.push(tab_2);
    let mut pane_2 = snap.panes[0].clone();
    pane_2.pane_id = "pane_2".into();
    pane_2.workspace_id = "ws_2".into();
    pane_2.tab_id = "tab_2".into();
    pane_2.focused = false;
    snap.panes.push(pane_2);
    snap.agents = vec![
        agent("pane_1", "ws_1", "tab_1"),
        agent("pane_2", "ws_2", "tab_2"),
    ];
    snap
}

fn state_with(config_toml: &str) -> ClientShellState {
    let config: Config = toml::from_str(config_toml).expect("config");
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(two_spaces()));
    state.set_pane_surface(surface());
    state
}

/// Trimmed text of the agents panel body, one entry per terminal row.
fn body_lines(state: &mut ClientShellState) -> Vec<String> {
    let frame = state.compose(106, 30).expect("frame");
    let body = state.hits.agent_body;
    let width = frame.width as usize;
    (body.y..body.bottom())
        .map(|y| {
            (body.x..body.right())
                .map(|x| frame.cells[y as usize * width + x as usize].symbol.as_str())
                .collect::<String>()
                .trim()
                .to_owned()
        })
        .collect()
}

#[test]
fn group_by_space_adds_a_header_row_per_space() {
    let mut state = state_with(
        r#"
[ui.sidebar.agents]
group_by_space = true
rows = [["state_text"]]
"#,
    );
    let lines = body_lines(&mut state);
    assert_eq!(
        &lines[..5],
        ["client-shell", "idle", "", "second", "idle"],
        "{lines:?}"
    );
    let body = state.hits.agent_body;
    let agent_rows = state
        .hits
        .agents
        .iter()
        .map(|(rect, pane_id)| (rect.y - body.y, pane_id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(agent_rows, [(1, "pane_1"), (4, "pane_2")]);
}

#[test]
fn group_by_space_off_keeps_flat_rows() {
    let mut state = state_with(
        r#"
[ui.sidebar.agents]
rows = [["state_text"]]
"#,
    );
    let lines = body_lines(&mut state);
    assert_eq!(&lines[..3], ["idle", "idle", ""], "{lines:?}");
    assert_eq!(state.hits.agents.len(), 2);
}

#[test]
fn group_by_space_is_ignored_in_priority_sort() {
    let mut state = state_with(
        r#"
[ui]
agent_panel_sort = "priority"

[ui.sidebar.agents]
group_by_space = true
rows = [["state_text"]]
"#,
    );
    let lines = body_lines(&mut state);
    assert_eq!(&lines[..3], ["idle", "idle", ""], "{lines:?}");
}

#[test]
fn grouped_rows_drop_the_workspace_token_but_priority_mode_keeps_it() {
    let rows = r#"rows = [["workspace"], ["state_text"]]"#;
    let mut grouped = state_with(&format!(
        "[ui.sidebar.agents]\ngroup_by_space = true\n{rows}\n"
    ));
    let lines = body_lines(&mut grouped);
    assert_eq!(
        &lines[..5],
        ["client-shell", "idle", "", "second", "idle"],
        "{lines:?}"
    );

    let mut priority = state_with(&format!(
        "[ui]\nagent_panel_sort = \"priority\"\n\n[ui.sidebar.agents]\ngroup_by_space = true\n{rows}\n"
    ));
    let lines = body_lines(&mut priority);
    assert_eq!(
        &lines[..4],
        ["client-shell", "idle", "second", "idle"],
        "{lines:?}"
    );
}

#[test]
fn grouped_rows_are_ordered_by_priority_within_and_across_spaces() {
    let mut snap = two_spaces();
    // ws_1 gains a working agent; ws_2's agent is blocked, so ws_2 comes first.
    let mut pane_3 = snap.panes[0].clone();
    pane_3.pane_id = "pane_3".into();
    pane_3.focused = false;
    snap.panes.push(pane_3);
    let mut working = agent("pane_3", "ws_1", "tab_1");
    working.agent_status = AgentStatus::Working;
    working.state_change_seq = 3;
    snap.agents.push(working);
    snap.agents[1].agent_status = AgentStatus::Blocked;
    snap.agents[1].state_change_seq = 2;

    let config: Config =
        toml::from_str("[ui.sidebar.agents]\ngroup_by_space = true\nrows = [[\"state_text\"]]\n")
            .expect("config");
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(snap));
    state.set_pane_surface(surface());
    let lines = body_lines(&mut state);
    assert_eq!(
        &lines[..6],
        ["second", "blocked", "", "client-shell", "working", "idle"],
        "{lines:?}"
    );
}
