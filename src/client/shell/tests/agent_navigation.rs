use super::*;

fn agent(pane_id: &str, name: &str, seq: u64, focused: bool) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: "ws_1".into(),
        tab_id: "tab_1".into(),
        name: Some(name.into()),
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

fn agents_snapshot() -> ClientShellSnapshot {
    let mut projected = snapshot();
    let mut second = projected.panes[0].clone();
    second.pane_id = "pane_2".into();
    second.focused = false;
    projected.panes.push(second);
    projected.agents = vec![
        agent("pane_1", "first", 1, true),
        agent("pane_2", "second", 2, false),
    ];
    projected
}

fn agent_state() -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(agents_snapshot()));
    state.set_pane_surface(surface());
    state.compose(106, 30).expect("initial frame");
    state
}

fn selected_pane(state: &ClientShellState) -> Option<&str> {
    state
        .navigate_agent
        .as_ref()
        .map(|target| target.pane_id.as_str())
}

fn agent_rect(state: &ClientShellState, pane_id: &str) -> Rect {
    state
        .hits
        .agents
        .iter()
        .find(|(_, hit)| hit == pane_id)
        .map(|(rect, _)| *rect)
        .expect("visible agent row")
}

fn enter_agent_navigation(state: &mut ClientShellState) {
    assert!(state.handle_input_bytes(&[0x02]).actions.is_empty());
    let outcome = state.handle_input_bytes(b"a");
    assert!(outcome.actions.is_empty());
    assert!(outcome.repaint);
    assert_eq!(state.mode, ClientShellMode::NavigateAgents);
}

#[test]
fn prefix_a_enters_agent_navigation_and_enter_focuses_selected_agent() {
    let mut state = agent_state();
    enter_agent_navigation(&mut state);
    assert_eq!(selected_pane(&state), Some("pane_1"));

    let down = state.handle_input_bytes(b"\x1b[B");
    assert!(down.actions.is_empty());
    assert_eq!(selected_pane(&state), Some("pane_2"));

    let frame = state.compose(106, 30).expect("agent navigation frame");
    let text = frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("AGENTS"), "{text}");
    let buffer = frame.to_ratatui_buffer().unwrap();
    let selected = agent_rect(&state, "pane_2");
    let focused = agent_rect(&state, "pane_1");
    let palette = &state.config.palette;
    let selection = if palette.selection_bg == ratatui::style::Color::Reset {
        palette.active_row_bg
    } else {
        palette.selection_bg
    };
    assert_eq!(buffer[(selected.x + 2, selected.y)].bg, selection);
    assert_eq!(buffer[(focused.x + 2, focused.y)].bg, palette.active_row_bg);

    let focus = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &focus.actions[..] else {
        panic!("selected agent should focus through endpoint API");
    };
    assert!(matches!(
        &request.method,
        crate::api::schema::Method::PaneFocus(target) if target.pane_id == "pane_2"
    ));
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.navigate_agent.is_none());
}

#[test]
fn escape_leaves_agent_navigation_without_focusing() {
    let mut state = agent_state();
    enter_agent_navigation(&mut state);
    let esc = state.handle_input_bytes(b"\x1b");
    assert!(esc.actions.is_empty());
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.navigate_agent.is_none());
}

#[test]
fn agent_selection_wraps_in_both_directions() {
    let mut state = agent_state();
    enter_agent_navigation(&mut state);
    assert_eq!(selected_pane(&state), Some("pane_1"));
    state.handle_input_bytes(b"\x1b[A");
    assert_eq!(selected_pane(&state), Some("pane_2"));
    state.handle_input_bytes(b"\x1b[B");
    assert_eq!(selected_pane(&state), Some("pane_1"));
}

#[test]
fn other_prefix_bindings_leave_agent_navigation_and_run() {
    let mut state = agent_state();
    enter_agent_navigation(&mut state);
    let switch = state.handle_input_bytes(b"w");
    assert!(switch.actions.is_empty());
    assert_eq!(state.mode, ClientShellMode::Navigate);
    assert!(state.navigate_agent.is_none());
    assert_eq!(
        state.navigate_workspace_id,
        state.navigation_target(&ClientEndpointId::Local, "ws_1")
    );
}

#[test]
fn enter_with_no_agents_exits_without_action() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    enter_agent_navigation(&mut state);
    assert_eq!(selected_pane(&state), None);
    let enter = state.handle_input_bytes(b"\r");
    assert!(enter.actions.is_empty());
    assert_eq!(state.mode, ClientShellMode::Terminal);
}
