//! "API bench" tab: the same queries via lat/lon and via geoid, CPU vs GPU.

use geodb_globe::api_bench::{Report, Timing, BATCH};
use geodb_globe::view::{fmt_coord, fmt_km};
use scopekit::ratatui::layout::{Constraint, Layout, Rect};
use scopekit::ratatui::style::{Color, Style, Stylize};
use scopekit::ratatui::text::{Line, Span};
use scopekit::ratatui::widgets::{Block, BorderType, Cell, Paragraph, Row, Table};
use scopekit::ratatui::Frame;

const ACCENT: Color = Color::Rgb(255, 184, 64);
const MUTED: Color = Color::Rgb(138, 151, 173);
const BORDER: Color = Color::Rgb(50, 62, 88);
const GOOD: Color = Color::Rgb(120, 220, 140);
const BAD: Color = Color::Rgb(255, 110, 110);

fn block(title: String) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(BORDER))
        .title(Span::raw(format!(" {title} ")).fg(ACCENT))
}

fn fmt_us(us: f64) -> String {
    if us < 1000.0 {
        format!("{us:.1} µs")
    } else {
        format!("{:.2} ms", us / 1000.0)
    }
}

fn speedup(base: f64, t: f64) -> Cell<'static> {
    let x = base / t.max(1e-9);
    let text = if x >= 10.0 {
        format!("{x:.0}×")
    } else {
        format!("{x:.1}×")
    };
    Cell::from(text).fg(if x >= 1.0 { GOOD } else { BAD })
}

fn accuracy(t: &Timing) -> Cell<'static> {
    if t.missed + t.extra == 0 {
        Cell::from("exact").fg(GOOD)
    } else {
        Cell::from(format!("−{} +{}", t.missed, t.extra)).fg(BAD)
    }
}

fn timing_table(title: String, rows: &[Timing], base: usize) -> Table<'static> {
    let base_us = rows.get(base).map_or(1.0, |t| t.us);
    let rows: Vec<Row> = rows
        .iter()
        .map(|t| {
            Row::new(vec![
                Cell::from(t.name),
                Cell::from(t.runs_on).fg(MUTED),
                Cell::from(fmt_us(t.us)),
                speedup(base_us, t.us),
                Cell::from(t.found.to_string()),
                accuracy(t),
            ])
        })
        .collect();
    Table::new(
        rows,
        [
            Constraint::Length(26),
            Constraint::Length(30),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Min(9),
        ],
    )
    .header(Row::new(["call", "runs on", "median", "vs lib", "found", "vs truth"]).fg(MUTED))
    .block(block(title))
}

pub fn draw(f: &mut Frame, area: Rect, report: Option<&Report>, running: bool) {
    let Some(r) = report else {
        let msg = if running {
            "Running benchmarks…"
        } else {
            "Press b to benchmark the API calls around the current view."
        };
        f.render_widget(Paragraph::new(msg).fg(MUTED), area);
        return;
    };
    let [head, single, batch, knn, foot] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(r.single.len() as u16 + 3),
        Constraint::Length(r.batch.len() as u16 + 3),
        Constraint::Length(r.knn.len() as u16 + 3),
        Constraint::Min(2),
    ])
    .areas(area);

    let status = if running { "  (running…)" } else { "" };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::raw("Centre ").fg(MUTED),
                Span::raw(fmt_coord(r.centre.0, r.centre.1)).fg(ACCENT),
                Span::raw(format!(
                    " · radius {} · {} cities · {} CPU threads",
                    fmt_km(r.radius_km),
                    r.cities,
                    r.threads
                ))
                .fg(MUTED),
                Span::raw(status).fg(ACCENT),
            ]),
            Line::from(vec![
                Span::raw("GPU ").fg(MUTED),
                Span::raw(r.gpu.clone().unwrap_or_else(|| "none".into())),
                Span::raw(format!(
                    " · geoids uploaded once in {:.1} ms ({} KB) · run took {:.0} ms",
                    r.gpu_upload_ms,
                    r.cities * 8 / 1024,
                    r.total_ms
                ))
                .fg(MUTED),
            ]),
            Line::raw(
                "b re-runs at the current view. Truth = brute-force f64 haversine on lat/lon.",
            )
            .fg(MUTED),
        ]),
        head,
    );

    f.render_widget(
        timing_table(
            format!("Radius search, 1 query ({})", fmt_km(r.radius_km)),
            &r.single,
            0,
        ),
        single,
    );

    let base_ms = r.batch.first().map_or(1.0, |b| b.total_ms);
    let evals = BATCH as f64 * r.cities as f64;
    let batch_rows: Vec<Row> = r
        .batch
        .iter()
        .map(|b| {
            Row::new(vec![
                Cell::from(b.name),
                Cell::from(b.runs_on).fg(MUTED),
                Cell::from(format!("{:.2} ms", b.total_ms)),
                speedup(base_ms, b.total_ms),
                Cell::from(format!("{:.0}", evals / (b.total_ms * 1e3))),
                if b.count_error == 0 {
                    Cell::from("exact").fg(GOOD)
                } else {
                    Cell::from(format!("±{}", b.count_error)).fg(BAD)
                },
            ])
        })
        .collect();
    f.render_widget(
        Table::new(
            batch_rows,
            [
                Constraint::Length(26),
                Constraint::Length(30),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(9),
                Constraint::Min(9),
            ],
        )
        .header(
            Row::new([
                "call",
                "runs on",
                "total",
                "vs lib",
                "M dist/s",
                "Σ|Δcount|",
            ])
            .fg(MUTED),
        )
        .block(block(format!(
            "Radius search, batch of {BATCH} queries ({} each)",
            fmt_km(r.batch_radius_km)
        ))),
        batch,
    );

    f.render_widget(
        timing_table("10 nearest cities, 1 query".into(), &r.knn, 0),
        knn,
    );

    f.render_widget(
        Paragraph::new(vec![
            Line::raw("GPU times are full round trips: write params, dispatch, copy back, wait."),
            Line::raw(
                "The GPU deinterleaves raw u64 geoids itself (WGSL has no u64: int Δ + f32).",
            ),
        ])
        .fg(MUTED),
        foot,
    );
}
