//! Fixture: fully token-driven, must produce zero findings.

fn clean_view(tokens: &Tokens) -> Element<'_, Message> {
    let a = text(label).size(theme::body_s(tokens, sc));
    let b = container(a).padding(tokens.spacing.md);
    let c = container(a).padding(Padding::from([tokens.spacing.sm, tokens.spacing.md]));
    let d = column![a, b].spacing(tokens.spacing.sm);
    let e = column![a, b].spacing(0);
    let f = container(a).style(|_t| container::Style {
        border: Border::default().rounded(tokens.radius.md),
        ..Default::default()
    });
    let g = to_iced_color(tokens.palette.accent);
    // Task 072: a centred row, and a deliberate alignment across lines.
    let h = hrow![a, b].spacing(tokens.spacing.sm);
    let i = row![
        text(label).size(theme::body_s(tokens, sc)),
        badge,
    ]
    .spacing(tokens.spacing.sm)
    .align_y(Alignment::Start);
    // Task 106: a control row wraps; a text-only row is out of the rule; a
    // builder is wrapped where it is used; a row that must not wrap says why.
    let j = hrow![text(label), button(text(x)).on_press(m)]
        .spacing(tokens.spacing.sm)
        .wrap();
    let k = hrow![text(label), text(value)].spacing(tokens.spacing.sm);
    let mut chips = hrow![text(label)].spacing(tokens.spacing.xs);
    chips = chips.push(chip);
    column.push(chips.wrap());
    // no-wrap: the inside of one button, not a row of controls
    let mut inner = hrow![].spacing(tokens.spacing.xs);
    // Task 118: a press that does not depend on a comparison with the current
    // value, and an `if` that has no press in it.
    let b = button(text(label)).on_press(m);
    if candidate != state.theme {
        log(candidate);
    }
    a
}
