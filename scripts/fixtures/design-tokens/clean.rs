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
    a
}
