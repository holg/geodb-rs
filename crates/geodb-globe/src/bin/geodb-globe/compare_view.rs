//! "Compare" tab: geoid-only location and distance vs. the stored coordinates.

use geodb_globe::compare::{self, Method};
use geodb_globe::geoid;
use geodb_globe::places::Nearby;
use geodb_globe::view::{fmt_coord, fmt_km, Query};
use scopekit::ratatui::layout::{Constraint, Layout, Rect};
use scopekit::ratatui::style::{Color, Style, Stylize};
use scopekit::ratatui::text::{Line, Span};
use scopekit::ratatui::widgets::{Block, BorderType, Cell, Paragraph, Row, Table};
use scopekit::ratatui::Frame;

/// Reference timing and per-method timings in ns per distance.
pub type Bench = (f64, [f64; Method::ALL.len()]);

const ACCENT: Color = Color::Rgb(255, 184, 64);
const MUTED: Color = Color::Rgb(138, 151, 173);
const BORDER: Color = Color::Rgb(50, 62, 88);

fn block(title: String) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(BORDER))
        .title(Span::raw(format!(" {title} ")).fg(ACCENT))
}

fn fmt_err(m: f64) -> String {
    if m < 0.001 {
        "< 1 mm".into()
    } else if m < 1.0 {
        format!("{:.1} mm", m * 1000.0)
    } else if m < 100.0 {
        format!("{m:.1} m")
    } else if m < 1000.0 {
        format!("{m:.0} m")
    } else {
        format!("{:.2} km", m / 1000.0)
    }
}

fn err_color(m: f64) -> Color {
    if m < 1.0 {
        Color::Rgb(120, 220, 140)
    } else if m < 100.0 {
        Color::Rgb(240, 210, 110)
    } else {
        Color::Rgb(255, 110, 110)
    }
}

fn err_cell(m: f64) -> Cell<'static> {
    Cell::from(fmt_err(m)).fg(err_color(m))
}

pub fn draw(
    f: &mut Frame,
    area: Rect,
    nearby: Option<&Nearby>,
    query: Option<Query>,
    selected: Option<usize>,
    bench: Option<&Bench>,
) {
    let (Some(n), Some((lat, lon, radius))) = (nearby, query) else {
        f.render_widget(Paragraph::new("No query yet.").fg(MUTED), area);
        return;
    };
    if n.places.is_empty() {
        f.render_widget(Paragraph::new("No cities to compare.").fg(MUTED), area);
        return;
    }
    let cmp = compare::compare((lat, lon), &n.places);
    let [head, loc, stats, rows] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(7),
        Constraint::Length(9),
        Constraint::Min(4),
    ])
    .areas(area);

    // ---- header
    let head_text = vec![
        Line::from(vec![
            Span::raw("Centre ").fg(MUTED),
            Span::raw(fmt_coord(lat, lon)).fg(ACCENT),
            Span::raw(format!(
                " · radius {} · {} cities compared",
                fmt_km(radius),
                cmp.rows.len()
            ))
            .fg(MUTED),
        ]),
        Line::from(vec![
            Span::raw("geoid ").fg(MUTED),
            Span::raw(format!("0x{:016x}", cmp.centre_geoid)),
            Span::raw("  u32 ").fg(MUTED),
            Span::raw(format!("0x{:08x}", geoid::truncate32(cmp.centre_geoid))),
        ]),
        Line::raw("Truth = f64 haversine on stored lat/lon. Methods only see the two geoids.")
            .fg(MUTED),
    ];
    f.render_widget(Paragraph::new(head_text), head);

    // ---- location of the selected (or first) place
    let idx = selected.filter(|&i| i < n.places.len()).unwrap_or(0);
    let p = &n.places[idx];
    let loc_rows = compare::location_rows(p.lat, p.lon, p.geoid)
        .into_iter()
        .map(|r| {
            Row::new(vec![
                Cell::from(r.source),
                Cell::from(format!("{:>2} B", r.bytes)).fg(MUTED),
                Cell::from(format!("{:>11.6}", r.lat)),
                Cell::from(format!("{:>11.6}", r.lon)),
                if r.bytes == 16 {
                    Cell::from("truth").fg(MUTED)
                } else {
                    err_cell(r.err_m)
                },
            ])
        });
    let loc_table = Table::new(
        loc_rows,
        [
            Constraint::Length(24),
            Constraint::Length(5),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(10),
        ],
    )
    .header(Row::new(["location from", "size", "lat", "lon", "error"]).fg(MUTED))
    .block(block(format!(
        "Where is {}? (geoid 0x{:016x})",
        p.name, p.geoid
    )));
    f.render_widget(loc_table, loc);

    // ---- error statistics per method
    let ns = |v: Option<f64>| v.map_or("–".into(), |v| format!("{v:.1}"));
    let mut stat_rows = vec![Row::new(vec![
        Cell::from("truth: haversine lat/lon").fg(MUTED),
        Cell::from(""),
        Cell::from(""),
        Cell::from(""),
        Cell::from(""),
        Cell::from(""),
        Cell::from(""),
        Cell::from(ns(bench.map(|b| b.0))),
        Cell::from("f64, 16 B/city").fg(MUTED),
    ])];
    for (k, s) in cmp.stats.iter().enumerate() {
        stat_rows.push(Row::new(vec![
            Cell::from(s.method.name()),
            err_cell(s.mean_m),
            err_cell(s.p99_m),
            err_cell(s.max_m),
            Cell::from(format!("{:.4}%", s.max_rel_pct)),
            Cell::from(format!("{:.1}%", s.order_kept * 100.0)),
            if s.nearest10_same {
                Cell::from("same").fg(Color::Rgb(120, 220, 140))
            } else {
                Cell::from("differs").fg(Color::Rgb(255, 110, 110))
            },
            Cell::from(ns(bench.map(|b| b.1[k]))),
            Cell::from(s.method.needs()).fg(MUTED),
        ]));
    }
    let stats_table = Table::new(
        stat_rows,
        [
            Constraint::Length(24),
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Min(10),
        ],
    )
    .header(
        Row::new([
            "distance by",
            "mean",
            "p99",
            "max",
            "max rel",
            "order",
            "10 near",
            "ns/op",
            "needs",
        ])
        .fg(MUTED),
    )
    .block(block(format!(
        "Distance centre → city: error vs truth ({} cities)",
        cmp.rows.len()
    )));
    f.render_widget(stats_table, stats);

    // ---- per-city distances, keeping the selection in view
    let visible = rows.height.saturating_sub(3) as usize;
    let start = selected
        .map(|s| s.saturating_sub(visible / 2))
        .unwrap_or(0)
        .min(cmp.rows.len().saturating_sub(visible));
    let city_rows = cmp
        .rows
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(i, r)| {
            let mut cells = vec![
                Cell::from(r.place.name.clone()),
                Cell::from(format!("{:>10.3}", r.real_km)),
            ];
            cells.extend(
                r.by_method
                    .iter()
                    .map(|&km| err_cell((km - r.real_km).abs() * 1000.0)),
            );
            let row = Row::new(cells);
            if Some(i) == selected {
                row.bg(Color::Rgb(90, 30, 64))
            } else {
                row
            }
        });
    let mut widths = vec![Constraint::Length(22), Constraint::Length(11)];
    widths.extend(Method::ALL.iter().map(|_| Constraint::Length(10)));
    let header: Vec<&str> = ["city", "truth km"]
        .into_iter()
        .chain(["f64 hav", "f32 hav", "f32 flat", "int flat", "u32 int"])
        .collect();
    let city_table = Table::new(city_rows, widths)
        .header(Row::new(header).fg(MUTED))
        .block(block("Per city: |method − truth|  (↑↓ in the list)".into()));
    f.render_widget(city_table, rows);
}
