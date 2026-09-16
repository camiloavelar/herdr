use super::*;

#[test]
fn sidebar_divider_sits_at_the_cell_edge() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let frame = state.compose(106, 30).expect("frame");
    let sidebar = state.layout(106, 30).sidebar;
    assert!(sidebar.width > 1);
    let divider_x = sidebar.right() - 1;
    let row = frame
        .cells
        .chunks(frame.width as usize)
        .nth(5)
        .expect("row");
    assert_eq!(row[divider_x as usize].symbol, "▕");
}
