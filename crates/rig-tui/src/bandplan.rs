//! The Ctrl-B band plan modal: one band at a time, stacked rows (privilege, ARRL, satellite)
//! under a frequency ruler, markers for the receive and transmit frequency and a cursor.
//!
//! The modal does no I/O and decides nothing about the rig: it turns keys into a [`BandAction`]
//! and `App` checks the rig and the foreground Activity before acting on it. Privilege is a soft
//! warning shown here and never a gate. The data is `hamtools_config::bandplan`, advisory only.

use chrono::NaiveDate;
use hamtools_config::LicenseClass;
use hamtools_config::bandplan::{BandPlan, Row};
use hamtools_tui::modal::Modal;
use logbook::Band;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::rig::RigMode;
use crate::style::dim;
use crate::ui::format_hz;

/// Data older than this many days shows a stale marker.
const STALE_AFTER_DAYS: i64 = 365;
/// Columns of row label at the left of every row.
const GUTTER: usize = 6;
const MAX_WIDTH: u16 = 78;

/// What a key asks `App` to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BandAction {
    Stay,
    Close,
    /// Set the receive frequency.
    SetRx(u64),
    /// Set the transmit frequency (split).
    SetTx(u64),
    /// Apply this mode (passband 0).
    SetMode(RigMode),
    /// `m` where the plan has no usual mode.
    NoMode,
}

/// What the modal needs to draw, from the App.
pub(crate) struct View {
    pub rx: Option<u64>,
    pub tx: Option<u64>,
    pub class: Option<LicenseClass>,
    pub today: NaiveDate,
}

pub(crate) struct BandplanModal {
    plan: &'static BandPlan,
    bands: Vec<Band>,
    index: usize,
    cursor: u64,
}

impl BandplanModal {
    /// Open on the band of `rx` (the nearest band below when outside all of them).
    pub(crate) fn open(rx: Option<u64>) -> Self {
        let plan = BandPlan::embedded();
        let bands = plan.bands();
        let index = rx.map_or(0, |hz| {
            bands
                .iter()
                .rposition(|b| b.lower_hz <= hz)
                .unwrap_or_default()
        });
        let mut modal = Self {
            plan,
            bands,
            index,
            cursor: 0,
        };
        modal.place_cursor(rx);
        modal
    }

    fn band(&self) -> Band {
        self.bands[self.index]
    }

    fn channels(&self) -> Vec<u64> {
        self.plan
            .channels
            .iter()
            .filter(|c| c.band == self.band().name)
            .map(|c| c.carrier_hz)
            .collect()
    }

    /// The frequency span the band is drawn over. 60 m is the channels and the contiguous
    /// segment, not the ADIF band's wide edges.
    fn range(&self) -> (u64, u64) {
        let band = self.band();
        let channels = self.channels();
        if channels.is_empty() {
            return (band.lower_hz, band.upper_hz);
        }
        let rows = self
            .plan
            .privilege
            .rows
            .iter()
            .filter(|r| r.band == band.name);
        let lo = channels
            .iter()
            .copied()
            .chain(rows.clone().map(|r| r.lo_hz))
            .min()
            .unwrap_or(band.lower_hz);
        let hi = channels
            .iter()
            .copied()
            .chain(rows.map(|r| r.hi_hz))
            .max()
            .unwrap_or(band.upper_hz);
        (lo.saturating_sub(2_000), hi + 2_000)
    }

    /// The cursor step: a 1-2-5 number near a sixtieth of the band.
    fn step(&self) -> u64 {
        let (lo, hi) = self.range();
        let target = ((hi - lo) / 60).max(1);
        let mut magnitude = 1;
        while magnitude * 10 <= target {
            magnitude *= 10;
        }
        [5, 2, 1]
            .into_iter()
            .map(|m| m * magnitude)
            .find(|&s| s <= target)
            .unwrap_or(1)
    }

    /// Start the cursor on the rig's frequency if it is in this band, else at the band's start;
    /// on 60 m, on the nearest channel.
    fn place_cursor(&mut self, rx: Option<u64>) {
        let (lo, hi) = self.range();
        let channels = self.channels();
        let inside = rx.filter(|hz| (lo..=hi).contains(hz));
        self.cursor = match (channels.is_empty(), inside) {
            (true, Some(hz)) => hz,
            (true, None) => lo,
            (false, Some(hz)) => channels
                .iter()
                .copied()
                .min_by_key(|c| c.abs_diff(hz))
                .unwrap_or(channels[0]),
            (false, None) => channels[0],
        };
    }

    fn arrl_at(&self, hz: u64) -> Option<&Row> {
        let band = self.band().name;
        self.plan
            .arrl
            .rows
            .iter()
            .find(|r| r.band == band && (r.lo_hz..=r.hi_hz).contains(&hz))
    }

    fn satellite_at(&self, hz: u64) -> Option<&Row> {
        let band = self.band().name;
        self.plan
            .satellite
            .rows
            .iter()
            .find(|r| r.band == band && (r.lo_hz..=r.hi_hz).contains(&hz))
    }

    fn move_cursor(&mut self, up: bool) {
        let channels = self.channels();
        if !channels.is_empty() {
            let next = if up {
                channels.iter().copied().find(|&c| c > self.cursor)
            } else {
                channels.iter().copied().rev().find(|&c| c < self.cursor)
            };
            if let Some(hz) = next {
                self.cursor = hz;
            }
            return;
        }
        let (lo, hi) = self.range();
        let step = self.step();
        let band = self.band().name;
        let starts = self
            .plan
            .arrl
            .rows
            .iter()
            .filter(|r| r.band == band)
            .map(|r| r.lo_hz);
        if up {
            let target = self.cursor.saturating_add(step).min(hi);
            // Land on a segment start the step would pass over.
            self.cursor = starts
                .filter(|&s| s > self.cursor && s <= target)
                .min()
                .unwrap_or(target);
        } else {
            let target = self.cursor.saturating_sub(step).max(lo);
            self.cursor = starts
                .filter(|&s| s < self.cursor && s >= target)
                .max()
                .unwrap_or(target);
        }
    }

    fn usual_mode(&self) -> Option<RigMode> {
        let mode = self.arrl_at(self.cursor)?.mode.as_deref()?;
        Some(match mode {
            "phone" if self.cursor < 10_000_000 => RigMode::Lsb,
            "phone" => RigMode::Usb,
            other => RigMode::parse(other),
        })
    }

    /// Handle a key. `rx` is the rig's receive frequency, used to place the cursor on a band
    /// change.
    pub(crate) fn on_key(&mut self, key: KeyEvent, rx: Option<u64>) -> BandAction {
        match key.code {
            KeyCode::Esc => BandAction::Close,
            KeyCode::Right | KeyCode::Char(']') => {
                if self.index + 1 < self.bands.len() {
                    self.index += 1;
                    self.place_cursor(rx);
                }
                BandAction::Stay
            }
            KeyCode::Left | KeyCode::Char('[') => {
                if self.index > 0 {
                    self.index -= 1;
                    self.place_cursor(rx);
                }
                BandAction::Stay
            }
            KeyCode::Up => {
                self.move_cursor(true);
                BandAction::Stay
            }
            KeyCode::Down => {
                self.move_cursor(false);
                BandAction::Stay
            }
            KeyCode::Enter => BandAction::SetRx(self.cursor),
            KeyCode::Char('T') => BandAction::SetTx(self.cursor),
            KeyCode::Char('m') => self
                .usual_mode()
                .map_or(BandAction::NoMode, BandAction::SetMode),
            _ => BandAction::Stay,
        }
    }

    /// `+` in privilege, `-` out of it, for the class, at `hz`.
    fn privileged(&self, hz: u64, class: LicenseClass) -> bool {
        self.plan.in_privilege(hz, class)
    }

    pub(crate) fn render(&self, frame: &mut Frame, area: Rect, view: &View) {
        let width = area.width.min(MAX_WIDTH);
        let bar_w = usize::from(width.saturating_sub(2))
            .saturating_sub(GUTTER)
            .max(8);
        let (lo, hi) = self.range();
        let band = self.band();

        let col_of = |hz: u64| -> usize {
            let at = u128::from(hz.clamp(lo, hi) - lo) * bar_w as u128 / u128::from(hi - lo);
            (at as usize).min(bar_w - 1)
        };
        let col_span = |c: usize| -> (u64, u64) {
            let span = u128::from(hi - lo);
            (
                lo + (c as u128 * span / bar_w as u128) as u64,
                lo + ((c as u128 + 1) * span / bar_w as u128) as u64,
            )
        };
        let rx_col = view.rx.filter(|h| (lo..=hi).contains(h)).map(col_of);
        let tx_col = view.tx.filter(|h| (lo..=hi).contains(h)).map(col_of);
        let cursor_col = col_of(self.cursor);

        // A row of cells with the receive column drawn reversed, so the marker crosses every row.
        let bar = |label: &str, cells: Vec<(char, Style)>| -> Line<'static> {
            let mut spans = vec![Span::styled(format!("{label:<GUTTER$}"), dim())];
            for (c, (ch, style)) in cells.into_iter().enumerate() {
                let style = if rx_col == Some(c) {
                    style.add_modifier(Modifier::REVERSED)
                } else {
                    style
                };
                spans.push(Span::styled(ch.to_string(), style));
            }
            Line::from(spans)
        };
        let plain = |label: &str, text: String| -> Line<'static> {
            Line::from(vec![
                Span::styled(format!("{label:<GUTTER$}"), dim()),
                Span::raw(text),
            ])
        };

        // Header and ruler.
        let header = Line::from(format!(
            "{}  {} - {}  advisory only",
            band.name,
            format_hz(lo),
            format_hz(hi)
        ));
        let (left, right) = (mhz(lo), mhz(hi));
        let ruler = if bar_w > left.len() + right.len() + 2 {
            format!(
                "{left}{}{right}",
                "-".repeat(bar_w - left.len() - right.len())
            )
        } else {
            "-".repeat(bar_w)
        };

        // Privilege row, or the note that replaces it.
        let privilege = match view.class {
            Some(class) => {
                let channels: Vec<_> = self
                    .plan
                    .channels
                    .iter()
                    .filter(|c| c.band == band.name)
                    .collect();
                let cells = (0..bar_w)
                    .map(|c| {
                        let (a, b) = col_span(c);
                        let in_priv = self.privileged((a + b) / 2, class)
                            || channels.iter().any(|ch| {
                                ch.classes.contains(&class) && (a..b).contains(&ch.carrier_hz)
                            });
                        if in_priv {
                            ('+', Style::default().fg(Color::Green))
                        } else {
                            ('-', Style::default().fg(Color::Red))
                        }
                    })
                    .collect();
                bar("Priv", cells)
            }
            None => plain("Priv", "hidden: set station.license_class".to_string()),
        };

        let layer_row = |label: &str, rows: &[Row], color: Color| -> Option<Line<'static>> {
            let rows: Vec<&Row> = rows.iter().filter(|r| r.band == band.name).collect();
            if rows.is_empty() {
                return None;
            }
            let mut cells = vec![(' ', Style::default()); bar_w];
            let style = Style::default().fg(color);
            for row in rows {
                let (c0, c1) = (col_of(row.lo_hz), col_of(row.hi_hz));
                for cell in &mut cells[c0..=c1] {
                    *cell = ('=', style);
                }
                if row.lo_hz == row.hi_hz {
                    cells[c0] = ('|', style);
                } else if row.label.chars().count() <= c1 - c0 + 1 {
                    for (i, ch) in row.label.chars().enumerate() {
                        cells[c0 + i] = (ch, style);
                    }
                }
            }
            Some(bar(label, cells))
        };
        let arrl = layer_row("ARRL", &self.plan.arrl.rows, Color::Cyan);
        let satellite = layer_row("Sat", &self.plan.satellite.rows, Color::Magenta);

        // Channels (60 m only).
        let channels = self.channels();
        let chan_row = (!channels.is_empty()).then(|| {
            let mut cells = vec![(' ', Style::default()); bar_w];
            for (i, hz) in channels.iter().enumerate() {
                cells[col_of(*hz)] = (char::from(b'1' + i as u8), Style::default());
            }
            bar("Chan", cells)
        });

        // Markers and cursor.
        let mut marks = vec![' '; bar_w];
        if let Some(c) = rx_col {
            marks[c] = 'R';
        }
        if let Some(c) = tx_col {
            marks[c] = if marks[c] == 'R' { '*' } else { 'T' };
        }
        let rig = plain("Rig", marks.into_iter().collect());
        let mut cur = vec![' '; bar_w];
        cur[cursor_col] = '^';
        let cursor = plain("Cur", cur.into_iter().collect());

        // Detail for the cursor.
        let priv_text = match view.class {
            None => "priv ?".to_string(),
            Some(class) if self.privileged(self.cursor, class) => "priv + in".to_string(),
            Some(class) => format!("priv - outside {class}"),
        };
        let detail = Line::from(format!(
            "{}  {priv_text}  plan: {}  sat: {}",
            format_hz(self.cursor),
            self.arrl_at(self.cursor).map_or("-", |r| label_of(r)),
            self.satellite_at(self.cursor).map_or("-", |r| label_of(r)),
        ));

        let keys = Line::styled(
            "Left/Right [ ] band  Up/Down cursor  Enter rx  m mode  T tx  Esc",
            dim(),
        );
        let as_of = self.plan.as_of();
        let stale = NaiveDate::parse_from_str(as_of, "%Y-%m-%d")
            .is_ok_and(|d| view.today.signed_duration_since(d).num_days() > STALE_AFTER_DAYS);
        let mut footer = format!("as_of {as_of}{}", if stale { " (stale)" } else { "" });
        if !channels.is_empty() {
            footer.push_str("  60 m channel data unverified");
        }
        let footer = Line::styled(footer, if stale { crate::style::yellow() } else { dim() });

        // Lines in drawing order. Dropped, when the screen is short, in this order: the ARRL row,
        // the satellite row, then the footer, key hints and detail. The ruler, privilege row,
        // markers and cursor always stay.
        let mut lines: Vec<(Rank, Line<'static>)> = vec![
            (Rank::Never, header),
            (Rank::Never, plain("", ruler)),
            (Rank::Never, privilege),
        ];
        lines.extend(chan_row.map(|l| (Rank::Never, l)));
        lines.extend(arrl.map(|l| (Rank::Arrl, l)));
        lines.extend(satellite.map(|l| (Rank::Sat, l)));
        lines.extend([
            (Rank::Never, rig),
            (Rank::Never, cursor),
            (Rank::Detail, detail),
            (Rank::Keys, keys),
            (Rank::AsOf, footer),
        ]);
        let available = usize::from(area.height.saturating_sub(2));
        for victim in [Rank::Arrl, Rank::Sat, Rank::AsOf, Rank::Keys, Rank::Detail] {
            if lines.len() <= available {
                break;
            }
            lines.retain(|(kind, _)| *kind != victim);
        }

        let height = (lines.len() + 2) as u16;
        let inner = Modal::new("Band plan", width, height).render(frame, area);
        let text: Vec<Line> = lines.into_iter().map(|(_, l)| l).collect();
        frame.render_widget(Paragraph::new(text), inner);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rank {
    Never,
    Arrl,
    Sat,
    AsOf,
    Keys,
    Detail,
}

fn label_of(row: &Row) -> &str {
    if row.label.is_empty() {
        "-"
    } else {
        &row.label
    }
}

/// `14.350`
fn mhz(hz: u64) -> String {
    format!("{}.{:03}", hz / 1_000_000, hz / 1_000 % 1_000)
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    /// The modal on 20 m, General, drawn into a `width` x `height` area.
    fn draw(width: u16, height: u16) -> Vec<String> {
        let modal = BandplanModal::open(Some(14_074_000));
        let view = View {
            rx: Some(14_074_000),
            tx: None,
            class: Some(LicenseClass::General),
            today: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
        };
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| modal.render(frame, frame.area(), &view))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(usize::from(width))
            .map(|row| row.iter().map(|c| c.symbol()).collect())
            .collect()
    }

    fn has(rows: &[String], needle: &str) -> bool {
        rows.iter().any(|r| r.contains(needle))
    }

    #[test]
    fn the_arrl_row_is_dropped_first_then_the_satellite_row() {
        let mut only_arrl_gone = false;
        for height in 6..=24 {
            let rows = draw(80, height);
            let (arrl, sat) = (has(&rows, "ARRL"), has(&rows, "Sat "));
            assert!(!(arrl && !sat), "ARRL kept after Sat dropped at {height}");
            only_arrl_gone |= sat && !arrl;
        }
        assert!(only_arrl_gone, "no height drops only the ARRL row");
        let full = draw(80, 24);
        assert!(has(&full, "ARRL") && has(&full, "Sat "));
    }

    #[test]
    fn privilege_marker_and_cursor_survive_the_smallest_height() {
        let rows = draw(80, 9);

        assert!(has(&rows, "Priv"), "{rows:#?}");
        assert!(has(&rows, "Rig"), "{rows:#?}");
        assert!(has(&rows, "Cur"), "{rows:#?}");
        assert!(!has(&rows, "ARRL") && !has(&rows, "Sat "), "{rows:#?}");
    }

    #[test]
    fn a_narrow_screen_cuts_lines_instead_of_wrapping_them() {
        let rows = draw(40, 24);

        assert_eq!(rows.iter().filter(|r| r.contains("as_of")).count(), 1);
        assert_eq!(rows.iter().filter(|r| r.contains("Left/Right")).count(), 1);
        assert_eq!(rows.iter().filter(|r| r.contains("Priv")).count(), 1);
    }
}
