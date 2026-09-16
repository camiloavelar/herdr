use super::*;

fn agent(pane_id: &str, seq: u64, focused: bool) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: "ws_1".into(),
        tab_id: "tab_1".into(),
        name: Some(pane_id.into()),
        display_agent: None,
        agent: None,
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: AgentStatus::Idle,
        state_change_seq: seq,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused,
    }
}

fn preview_snapshot() -> ClientShellSnapshot {
    let mut projected = snapshot();
    let mut second = projected.workspaces[0].clone();
    second.workspace_id = "ws_2".into();
    second.number = 2;
    second.focused = false;
    projected.workspaces.push(second);
    let mut pane = projected.panes[0].clone();
    pane.pane_id = "pane_2".into();
    pane.focused = false;
    projected.panes.push(pane);
    projected.agents = vec![agent("pane_1", 1, true), agent("pane_2", 2, false)];
    projected
}

fn preview_state(enabled: bool) -> ClientShellState {
    let mut config = Config::default();
    config.ui.navigation_preview = enabled;
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(preview_snapshot()));
    state.set_pane_surface(surface());
    state.compose(106, 30).expect("initial frame");
    state
}

fn workspace_focus_ids(outcome: &ClientShellInput) -> Vec<String> {
    outcome
        .actions
        .iter()
        .filter_map(|action| match action {
            ClientShellAction::Endpoint { request, .. } => match &request.method {
                crate::api::schema::Method::WorkspaceFocus(target) => {
                    Some(target.workspace_id.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn pane_focus_ids(outcome: &ClientShellInput) -> Vec<String> {
    outcome
        .actions
        .iter()
        .filter_map(|action| match action {
            ClientShellAction::Endpoint { request, .. } => match &request.method {
                crate::api::schema::Method::PaneFocus(target) => Some(target.pane_id.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn workspace_navigation_previews_selection_and_restores_origin_on_escape() {
    let mut state = preview_state(true);
    state.handle_input_bytes(&[0x02]);
    state.handle_input_bytes(b"w");
    assert_eq!(state.mode, ClientShellMode::Navigate);

    let down = state.handle_input_bytes(b"\x1b[B");
    assert_eq!(workspace_focus_ids(&down), vec!["ws_2"]);

    let esc = state.handle_input_bytes(b"\x1b");
    assert_eq!(workspace_focus_ids(&esc), vec!["ws_1"]);
    assert_eq!(state.mode, ClientShellMode::Terminal);
}

#[test]
fn workspace_navigation_enter_keeps_previewed_focus() {
    let mut state = preview_state(true);
    state.handle_input_bytes(&[0x02]);
    state.handle_input_bytes(b"w");
    state.handle_input_bytes(b"\x1b[B");

    let enter = state.handle_input_bytes(b"\r");
    assert_eq!(workspace_focus_ids(&enter), vec!["ws_2"]);
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.navigation_preview_origin.is_none());
}

#[test]
fn workspace_navigation_does_not_preview_when_disabled() {
    let mut state = preview_state(false);
    state.handle_input_bytes(&[0x02]);
    state.handle_input_bytes(b"w");
    let down = state.handle_input_bytes(b"\x1b[B");
    assert!(down.actions.is_empty());
    let esc = state.handle_input_bytes(b"\x1b");
    assert!(esc.actions.is_empty());
}

#[test]
fn workspace_navigation_escape_without_moving_sends_nothing() {
    let mut state = preview_state(true);
    state.handle_input_bytes(&[0x02]);
    state.handle_input_bytes(b"w");
    let esc = state.handle_input_bytes(b"\x1b");
    assert!(esc.actions.is_empty());
}

#[test]
fn agent_navigation_previews_selection_and_restores_origin_on_escape() {
    let mut state = preview_state(true);
    state.handle_input_bytes(&[0x02]);
    state.handle_input_bytes(b"a");
    assert_eq!(state.mode, ClientShellMode::NavigateAgents);

    let down = state.handle_input_bytes(b"\x1b[B");
    assert_eq!(pane_focus_ids(&down), vec!["pane_2"]);

    let esc = state.handle_input_bytes(b"\x1b");
    assert_eq!(pane_focus_ids(&esc), vec!["pane_1"]);
    assert_eq!(state.mode, ClientShellMode::Terminal);
}

#[test]
fn agent_navigation_enter_keeps_previewed_focus() {
    let mut state = preview_state(true);
    state.handle_input_bytes(&[0x02]);
    state.handle_input_bytes(b"a");
    state.handle_input_bytes(b"\x1b[B");

    let enter = state.handle_input_bytes(b"\r");
    assert_eq!(pane_focus_ids(&enter), vec!["pane_2"]);
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.navigation_preview_origin.is_none());
}

#[test]
fn agent_navigation_does_not_preview_when_disabled() {
    let mut state = preview_state(false);
    state.handle_input_bytes(&[0x02]);
    state.handle_input_bytes(b"a");
    let down = state.handle_input_bytes(b"\x1b[B");
    assert!(down.actions.is_empty());
}
