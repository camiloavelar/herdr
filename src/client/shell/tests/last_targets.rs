use super::*;

/// Two spaces: ws_1 (tab_1, tab_1b) and ws_2 (tab_2). `focus` names the focused tab.
fn projected(revision: u64, focus: &str) -> ClientShellSnapshot {
    let mut snap = snapshot();
    snap.revision = revision;
    let base_ws = snap.workspaces[0].clone();
    let base_tab = snap.tabs[0].clone();
    let workspace_of = |tab: &str| if tab == "tab_2" { "ws_2" } else { "ws_1" };
    snap.workspaces = ["ws_1", "ws_2"]
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let mut ws = base_ws.clone();
            ws.workspace_id = (*id).into();
            ws.number = index + 1;
            ws.focused = workspace_of(focus) == *id;
            ws.active_tab_id = if *id == "ws_2" {
                "tab_2".into()
            } else if focus == "tab_1b" {
                "tab_1b".into()
            } else {
                "tab_1".into()
            };
            ws
        })
        .collect();
    snap.tabs = ["tab_1", "tab_1b", "tab_2"]
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let mut tab = base_tab.clone();
            tab.tab_id = (*id).into();
            tab.workspace_id = workspace_of(id).into();
            tab.number = index + 1;
            tab.focused = *id == focus;
            tab
        })
        .collect();
    snap.focused_workspace_id = Some(workspace_of(focus).into());
    snap.focused_tab_id = Some(focus.into());
    snap
}

fn state_focused_on(focus: &str) -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(projected(1, focus)));
    state
}

fn focus(state: &mut ClientShellState, revision: u64, tab: &str) {
    state.set_snapshot(Box::new(projected(revision, tab)));
}

fn run(state: &mut ClientShellState, action: crate::input::KeybindAction) -> Option<String> {
    let mut outcome = ClientShellInput::default();
    state.record_binding(crate::input::KeybindMatch::Action(action), &mut outcome);
    match outcome.actions.as_slice() {
        [] => None,
        [ClientShellAction::Endpoint { request, .. }] => match &request.method {
            crate::api::schema::Method::WorkspaceFocus(target) => {
                Some(format!("workspace:{}", target.workspace_id))
            }
            crate::api::schema::Method::TabFocus(target) => Some(format!("tab:{}", target.tab_id)),
            other => panic!("unexpected method {other:?}"),
        },
        other => panic!("unexpected actions {other:?}"),
    }
}

#[test]
fn last_workspace_toggles_between_the_two_most_recent_spaces() {
    let mut state = state_focused_on("tab_1");
    assert_eq!(
        run(&mut state, crate::input::KeybindAction::LastWorkspace),
        None
    );

    focus(&mut state, 2, "tab_2");
    assert_eq!(
        run(&mut state, crate::input::KeybindAction::LastWorkspace),
        Some("workspace:ws_1".into())
    );

    focus(&mut state, 3, "tab_1");
    assert_eq!(
        run(&mut state, crate::input::KeybindAction::LastWorkspace),
        Some("workspace:ws_2".into())
    );
}

#[test]
fn last_workspace_ignores_a_space_that_no_longer_exists() {
    let mut state = state_focused_on("tab_1");
    focus(&mut state, 2, "tab_2");
    let mut without_ws_1 = projected(3, "tab_2");
    without_ws_1
        .workspaces
        .retain(|ws| ws.workspace_id != "ws_1");
    without_ws_1.tabs.retain(|tab| tab.workspace_id != "ws_1");
    state.set_snapshot(Box::new(without_ws_1));
    assert_eq!(
        run(&mut state, crate::input::KeybindAction::LastWorkspace),
        None
    );
}

#[test]
fn last_tab_toggles_within_the_current_space() {
    let mut state = state_focused_on("tab_1");
    assert_eq!(run(&mut state, crate::input::KeybindAction::LastTab), None);

    focus(&mut state, 2, "tab_1b");
    assert_eq!(
        run(&mut state, crate::input::KeybindAction::LastTab),
        Some("tab:tab_1".into())
    );

    focus(&mut state, 3, "tab_1");
    assert_eq!(
        run(&mut state, crate::input::KeybindAction::LastTab),
        Some("tab:tab_1b".into())
    );
}

#[test]
fn last_tab_is_remembered_per_space() {
    let mut state = state_focused_on("tab_1");
    focus(&mut state, 2, "tab_1b");
    // Switching spaces does not count as a tab change inside ws_1 or ws_2.
    focus(&mut state, 3, "tab_2");
    assert_eq!(run(&mut state, crate::input::KeybindAction::LastTab), None);
    focus(&mut state, 4, "tab_1b");
    assert_eq!(
        run(&mut state, crate::input::KeybindAction::LastTab),
        Some("tab:tab_1".into())
    );
}

#[test]
fn doubled_prefix_runs_a_user_binding_instead_of_sending_the_prefix() {
    let config: Config = toml::from_str(
        r#"
[keys]
last_tab = "prefix+ctrl+b"
"#,
    )
    .expect("config");
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(projected(1, "tab_1")));
    state.set_pane_surface(surface());
    focus(&mut state, 2, "tab_1b");

    assert!(state.handle_input_bytes(&[0x02]).actions.is_empty());
    let second = state.handle_input_bytes(&[0x02]);
    assert!(
        second.requests.is_empty(),
        "literal prefix must not reach the pane"
    );
    assert!(matches!(
        second.actions.as_slice(),
        [ClientShellAction::Endpoint { request, .. }]
            if matches!(&request.method, crate::api::schema::Method::TabFocus(t) if t.tab_id == "tab_1")
    ));
}

#[test]
fn doubled_prefix_still_sends_the_prefix_without_a_binding() {
    let mut state = state_focused_on("tab_1");
    state.set_pane_surface(surface());
    state.handle_input_bytes(&[0x02]);
    let second = state.handle_input_bytes(&[0x02]);
    assert!(second.actions.is_empty());
    assert!(
        !second.requests.is_empty(),
        "literal prefix should go to the pane"
    );
}
