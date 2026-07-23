//! Interaction state for the inspect TUI: active tab and column scrolling.

/// The three inspect tabs, in display order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Metadata,
    Columns,
    Map,
}

const TAB_ORDER: [Tab; 3] = [Tab::Metadata, Tab::Columns, Tab::Map];

/// Interaction state, independent of any terminal backend.
pub struct App {
    pub tab: Tab,
    pub column_offset: usize,
    pub column_count: usize,
    pub should_quit: bool,
}

impl App {
    pub fn new(column_count: usize) -> Self {
        App {
            tab: Tab::Metadata,
            column_offset: 0,
            column_count,
            should_quit: false,
        }
    }

    fn tab_index(&self) -> usize {
        TAB_ORDER.iter().position(|t| *t == self.tab).unwrap_or(0)
    }

    pub fn next_tab(&mut self) {
        self.tab = TAB_ORDER[(self.tab_index() + 1) % TAB_ORDER.len()];
    }

    pub fn prev_tab(&mut self) {
        self.tab = TAB_ORDER[(self.tab_index() + TAB_ORDER.len() - 1) % TAB_ORDER.len()];
    }

    /// Largest valid scroll offset (0 when there are no columns).
    fn max_offset(&self) -> usize {
        self.column_count.saturating_sub(1)
    }

    pub fn scroll_down(&mut self) {
        self.column_offset = (self.column_offset + 1).min(self.max_offset());
    }

    pub fn scroll_up(&mut self) {
        self.column_offset = self.column_offset.saturating_sub(1);
    }

    pub fn to_top(&mut self) {
        self.column_offset = 0;
    }

    pub fn to_bottom(&mut self) {
        self.column_offset = self.max_offset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_navigation_wraps_both_directions() {
        let mut app = App::new(10);
        assert_eq!(app.tab, Tab::Metadata);
        app.next_tab();
        assert_eq!(app.tab, Tab::Columns);
        app.next_tab();
        assert_eq!(app.tab, Tab::Map);
        app.next_tab();
        assert_eq!(app.tab, Tab::Metadata); // wrapped forward
        app.prev_tab();
        assert_eq!(app.tab, Tab::Map); // wrapped backward
    }

    #[test]
    fn scroll_is_clamped_to_column_range() {
        let mut app = App::new(3); // valid offsets 0..=2
        app.scroll_up(); // already at top, stays 0
        assert_eq!(app.column_offset, 0);
        app.scroll_down();
        app.scroll_down();
        app.scroll_down(); // would be 3, clamps to 2
        assert_eq!(app.column_offset, 2);
        app.to_top();
        assert_eq!(app.column_offset, 0);
        app.to_bottom();
        assert_eq!(app.column_offset, 2);
    }

    #[test]
    fn scroll_on_empty_columns_stays_zero() {
        let mut app = App::new(0);
        app.scroll_down();
        app.to_bottom();
        assert_eq!(app.column_offset, 0);
    }
}
