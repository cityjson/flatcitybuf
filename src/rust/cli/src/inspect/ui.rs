//! `ratatui` rendering of the three inspect tabs.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Points, Rectangle};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Tabs};
use ratatui::Frame;

use crate::inspect::app::{App, Tab};
use crate::inspect::map;
use crate::inspect::model::InspectModel;

/// Render the tab bar plus the body of the active tab.
pub fn draw(frame: &mut Frame, model: &InspectModel, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(frame.area());

    draw_tab_bar(frame, chunks[0], app);
    match app.tab {
        Tab::Metadata => draw_metadata(frame, chunks[1], model),
        Tab::Columns => draw_columns(frame, chunks[1], model, app),
        Tab::Map => draw_map(frame, chunks[1], model),
    }
}

fn draw_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    let titles = ["Metadata", "Columns", "Map"];
    let selected = app.tab_index();
    let tabs = Tabs::new(titles.iter().map(|t| Line::from(*t)).collect::<Vec<_>>())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Header Categories"),
        )
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, area);
}

fn kv(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(value),
    ])
}

fn draw_metadata(frame: &mut Frame, area: Rect, model: &InspectModel) {
    let mut lines: Vec<Line> = Vec::new();
    if let Some(t) = &model.title {
        lines.push(kv("Title", t.clone()));
    }
    if let Some(id) = &model.identifier {
        lines.push(kv("Identifier", id.clone()));
    }
    lines.push(kv("CityJSON Version", model.version.clone()));
    lines.push(kv("Features", model.features_count.to_string()));
    lines.push(kv("Columns", model.columns.len().to_string()));
    lines.push(kv(
        "Spatial Index R-Tree Node Size",
        model.index_node_size.to_string(),
    ));
    lines.push(kv(
        "Attribute Indices",
        model.attribute_index_count.to_string(),
    ));
    if let Some(d) = &model.reference_date {
        lines.push(kv("Reference Date", d.clone()));
    }
    if let Some(e) = &model.extent {
        lines.push(kv(
            "Bounds",
            format!(
                "[{:.4}, {:.4}, {:.4}] .. [{:.4}, {:.4}, {:.4}]",
                e.min[0], e.min[1], e.min[2], e.max[0], e.max[1], e.max[2]
            ),
        ));
        let d = e.dimensions();
        lines.push(kv(
            "Dimensions",
            format!("{:.2} x {:.2} x {:.2}", d[0], d[1], d[2]),
        ));
    }
    if let Some(t) = &model.transform {
        lines.push(kv(
            "Scale",
            format!("[{:.6}, {:.6}, {:.6}]", t.scale[0], t.scale[1], t.scale[2]),
        ));
        lines.push(kv(
            "Translate",
            format!(
                "[{:.3}, {:.3}, {:.3}]",
                t.translate[0], t.translate[1], t.translate[2]
            ),
        ));
    }
    match &model.crs {
        Some(c) => {
            lines.push(kv("CRS Code", c.code_label()));
            if c.version != 0 {
                lines.push(kv("CRS Version", c.version.to_string()));
            }
            if let Some(cs) = &c.code_string {
                lines.push(kv("CRS Code String", cs.clone()));
            }
        }
        None => lines.push(kv("CRS", "Not set".into())),
    }

    let para =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Metadata"));
    frame.render_widget(para, area);
}

fn draw_columns(frame: &mut Frame, area: Rect, model: &InspectModel, app: &App) {
    let header = Row::new([
        "Name",
        "Type",
        "Description",
        "Nullable",
        "Primary Key",
        "Unique",
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows = model.columns.iter().enumerate().map(|(i, c)| {
        let style = if i == app.column_offset {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Row::new(vec![
            Cell::from(c.name.clone()),
            Cell::from(c.type_name.clone()),
            Cell::from(c.description.clone().unwrap_or_else(|| "-".into())),
            Cell::from(c.nullable.to_string()),
            Cell::from(c.primary_key.to_string()),
            Cell::from(c.unique.to_string()),
        ])
        .style(style)
    });
    let widths = [
        Constraint::Percentage(28),
        Constraint::Percentage(12),
        Constraint::Percentage(28),
        Constraint::Percentage(11),
        Constraint::Percentage(12),
        Constraint::Percentage(9),
    ];
    let title = if model.columns.is_empty() {
        "Columns (0)".to_string()
    } else {
        format!(
            "Columns ({} of {})",
            app.column_offset + 1,
            model.columns.len()
        )
    };
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title));
    let mut state = TableState::default();
    state.select(Some(app.column_offset));
    frame.render_stateful_widget(table, area, &mut state);
}

fn draw_map(frame: &mut Frame, area: Rect, model: &InspectModel) {
    let extent = match &model.extent {
        Some(e) => e,
        None => {
            let para = Paragraph::new("No geographical extent in header.")
                .block(Block::default().borders(Borders::ALL).title("Map"));
            frame.render_widget(para, area);
            return;
        }
    };

    if !map::is_geographic(model.crs.as_ref(), extent) {
        let crs = model
            .crs
            .as_ref()
            .map(|c| c.code_label())
            .unwrap_or_else(|| "unknown".into());
        let msg = format!(
            "Map unavailable: projected CRS ({crs}).\nExtent: [{:.2}, {:.2}] .. [{:.2}, {:.2}]",
            extent.min[0], extent.min[1], extent.max[0], extent.max[1]
        );
        let para = Paragraph::new(msg).block(Block::default().borders(Borders::ALL).title("Map"));
        frame.render_widget(para, area);
        return;
    }

    let coast: &'static [(f64, f64)] = map::coastline_points();
    let (min_x, min_y, max_x, max_y) = (extent.min[0], extent.min[1], extent.max[0], extent.max[1]);
    let crs_label = model
        .crs
        .as_ref()
        .map(|c| c.code_label())
        .unwrap_or_else(|| "geographic".to_string());
    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Extent of Data ({crs_label})")),
        )
        .x_bounds([-180.0, 180.0])
        .y_bounds([-90.0, 90.0])
        .paint(move |ctx| {
            ctx.draw(&Points {
                coords: coast,
                color: Color::Rgb(200, 90, 40),
            });
            ctx.draw(&Rectangle {
                x: min_x,
                y: min_y,
                width: (max_x - min_x).max(0.5),
                height: (max_y - min_y).max(0.5),
                color: Color::Green,
            });
        });
    frame.render_widget(canvas, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::app::App;
    use crate::inspect::model::{ColumnInfo, CrsInfo, ExtentInfo, InspectModel};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn sample_model() -> InspectModel {
        InspectModel {
            title: Some("Sample City".into()),
            identifier: None,
            version: "2.0".into(),
            features_count: 42,
            reference_date: None,
            index_node_size: 16,
            attribute_index_count: 1,
            columns: vec![ColumnInfo {
                name: "building_height".into(),
                type_name: "Double".into(),
                description: None,
                nullable: true,
                primary_key: false,
                unique: false,
            }],
            crs: None,
            extent: None,
            transform: None,
        }
    }

    /// Sample model with a geographic (EPSG:4326) CRS and a lon/lat extent,
    /// so `draw_map` takes the Canvas branch.
    fn geographic_model() -> InspectModel {
        let mut model = sample_model();
        model.extent = Some(ExtentInfo {
            min: [4.0, 52.0, 0.0],
            max: [5.0, 53.0, 10.0],
        });
        model.crs = Some(CrsInfo {
            authority: Some("EPSG".into()),
            code: 4326,
            version: 0,
            code_string: None,
        });
        model
    }

    /// Sample model with a projected (EPSG:28992) CRS and a metres extent, so
    /// `draw_map` falls back to the "Map unavailable" message.
    fn projected_model() -> InspectModel {
        let mut model = sample_model();
        model.extent = Some(ExtentInfo {
            min: [84000.0, 447000.0, 0.0],
            max: [85000.0, 448000.0, 10.0],
        });
        model.crs = Some(CrsInfo {
            authority: Some("EPSG".into()),
            code: 28992,
            version: 0,
            code_string: None,
        });
        model
    }

    fn render(model: &InspectModel, app: &App) -> String {
        let backend = TestBackend::new(90, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, model, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        buf.content().iter().map(|c| c.symbol()).collect()
    }

    fn rendered_text(app: &App) -> String {
        render(&sample_model(), app)
    }

    #[test]
    fn metadata_tab_shows_title_and_tab_bar() {
        let app = App::new(1); // defaults to Metadata
        let text = rendered_text(&app);
        assert!(text.contains("Metadata"));
        assert!(text.contains("Columns"));
        assert!(text.contains("Map"));
        assert!(text.contains("Sample City"));
    }

    #[test]
    fn columns_tab_shows_column_name() {
        let mut app = App::new(1);
        app.next_tab(); // Columns
        let text = rendered_text(&app);
        assert!(text.contains("building_height"));
    }

    fn many_columns_model(count: usize) -> InspectModel {
        let mut model = sample_model();
        model.columns = (0..count)
            .map(|i| ColumnInfo {
                name: format!("col_{i:02}"),
                type_name: "Double".into(),
                description: None,
                nullable: true,
                primary_key: false,
                unique: false,
            })
            .collect();
        model
    }

    #[test]
    fn columns_tab_viewport_follows_selection_when_scrolled_past_visible_rows() {
        let model = many_columns_model(40);
        let mut app = App::new(model.columns.len());
        app.next_tab(); // Columns
        app.to_bottom(); // column_offset -> 39, the last column

        // Short backend: only a handful of rows fit (header + block borders
        // + tab bar leave room for a few data rows), so the plain-Table bug
        // (always rendering from row 0) would still show early columns and
        // never reach col_39.
        let backend = TestBackend::new(90, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &model, &app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();

        assert!(
            text.contains("col_39"),
            "expected the selected last column to be visible after scrolling to the bottom"
        );
        assert!(
            !text.contains("col_00"),
            "expected the viewport to have scrolled past the first column"
        );
    }

    #[test]
    fn map_tab_renders_canvas_for_geographic_crs() {
        let model = geographic_model();
        let mut app = App::new(model.columns.len());
        app.next_tab(); // Columns
        app.next_tab(); // Map
        let text = render(&model, &app);
        assert!(text.contains("Extent of Data"));
        assert!(!text.contains("Map unavailable"));
    }

    #[test]
    fn map_tab_shows_unavailable_message_for_projected_crs() {
        let model = projected_model();
        let mut app = App::new(model.columns.len());
        app.next_tab(); // Columns
        app.next_tab(); // Map
        let text = render(&model, &app);
        assert!(text.contains("Map unavailable"));
    }
}
