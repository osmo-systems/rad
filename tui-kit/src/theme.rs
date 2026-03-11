use ratatui::style::{Color, Modifier, Style};

/// Color and style palette for the application.
///
/// Create a [`Theme::default()`] for the standard lazygit-inspired scheme,
/// or build a custom one by modifying individual fields.
///
/// `Theme` is `Copy` so it is cheap to pass by value or reference.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Border of the currently-focused interactive panel.
    pub border_focused: Style,
    /// Border of passive / display-only panels.
    pub border_unfocused: Style,
    /// Border of floating popups (always treated as focused).
    pub border_popup: Style,
    /// Border of error popups.
    pub border_error: Style,
    /// Border of warning popups.
    pub border_warning: Style,

    /// Label of the active tab.
    pub tab_active: Style,
    /// Label of an inactive tab.
    pub tab_inactive: Style,

    /// Style applied to the selected row in a list (fg + bg combined).
    pub selection: Style,

    /// Inline keyboard shortcut labels (e.g. "Enter", "Tab").
    pub shortcut_key: Style,
    /// The `-[n]-` digit indicator shown in widget titles.
    pub shortcut_indicator: Style,

    /// Section / group headers inside popups.
    pub section_header: Style,
    /// Normal body text.
    pub body: Style,
    /// Dimmed hint and footer text.
    pub hint: Style,
    /// Separator lines (─ characters).
    pub separator: Style,
}

impl Default for Theme {
    /// The standard lazygit-inspired palette used across rad projects.
    ///
    /// | Role               | Color        |
    /// |--------------------|--------------|
    /// | Focused border     | Green + Bold |
    /// | Unfocused border   | White        |
    /// | Active tab         | Green + Bold |
    /// | Inactive tab       | White        |
    /// | Selection          | White on Blue + Bold |
    /// | Shortcut keys      | Yellow       |
    /// | Section headers    | Cyan + Bold  |
    /// | Hints              | Dark gray    |
    fn default() -> Self {
        Self {
            border_focused:   Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            border_unfocused: Style::default().fg(Color::White),
            border_popup:     Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            border_error:     Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            border_warning:   Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),

            tab_active:   Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            tab_inactive: Style::default().fg(Color::White),

            selection: Style::default()
                .fg(Color::Indexed(7))
                .bg(Color::Indexed(6))
                .add_modifier(Modifier::BOLD),

            shortcut_key:       Style::default().fg(Color::Yellow),
            shortcut_indicator: Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),

            section_header: Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            body:           Style::default().fg(Color::White),
            hint:           Style::default().fg(Color::DarkGray),
            separator:      Style::default().fg(Color::Green),
        }
    }
}
