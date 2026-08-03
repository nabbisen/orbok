//! Fixture: one planted violation per RFC-052 §5 category.

fn violation_view(tokens: &Tokens) -> Element<'_, Message> {
    // Category 1: literal font size.
    let a = text(label).size(12);
    // Category 2: literal bare padding.
    let b = container(a).padding(10);
    // Category 3: literal array padding.
    let c = container(a).padding(Padding::from([12.0, 16.0]));
    // Category 4: literal non-zero spacing.
    let d = column![a, b].spacing(8);
    // Category 5: literal radius.
    let e = container(a).style(|_t| container::Style {
        border: Border::default().rounded(12.0),
        ..Default::default()
    });
    // Category 6: hard-coded colour.
    let f = iced::Color::from_rgb(0.2, 0.4, 0.6);
    a
}
