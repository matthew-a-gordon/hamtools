use hamtools_tui::modal::{Modal, centred};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

#[test]
fn centred_rect_is_clamped_to_the_area() {
    assert_eq!(
        centred(Rect::new(0, 0, 80, 24), 40, 10),
        Rect::new(20, 7, 40, 10)
    );
    assert_eq!(
        centred(Rect::new(0, 0, 30, 5), 40, 10),
        Rect::new(0, 0, 30, 5)
    );
}

#[test]
fn modal_clears_what_is_underneath_and_draws_title_and_border() {
    let mut terminal = Terminal::new(TestBackend::new(20, 7)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new(vec!["x".repeat(20); 7].join("\n")),
                frame.area(),
            );
            let inner = Modal::new("Hi", 10, 3).render(frame, frame.area());
            assert_eq!(inner, Rect::new(6, 3, 8, 1));
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..7)
        .map(|y| (0..20).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    assert_eq!(
        rows,
        [
            "xxxxxxxxxxxxxxxxxxxx",
            "xxxxxxxxxxxxxxxxxxxx",
            "xxxxx┏ Hi ━━━━┓xxxxx",
            "xxxxx┃        ┃xxxxx",
            "xxxxx┗━━━━━━━━┛xxxxx",
            "xxxxxxxxxxxxxxxxxxxx",
            "xxxxxxxxxxxxxxxxxxxx",
        ]
    );
}
