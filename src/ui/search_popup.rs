use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Padding, Paragraph},
    Frame,
};
use std::time::Instant;

use rad::search::{detect_context, parse_query, parser, AutocompleteContext, ParseError};
use tui_kit::Theme;

pub struct SearchPopup {
    pub input: String,
    pub cursor_position: usize,
    pub parse_error: Option<ParseError>,
    pub error_debounce_timer: Option<Instant>,
    pub autocomplete_shown: bool,
    pub autocomplete_items: Vec<String>,
    pub autocomplete_selected: usize,
    pub autocomplete_scroll_offset: usize,
    pub autocomplete_context: AutocompleteContext,
}

impl SearchPopup {
    pub fn new(initial_input: String) -> Self {
        let cursor_position = initial_input.len();
        Self {
            input: initial_input,
            cursor_position,
            parse_error: None,
            error_debounce_timer: None,
            autocomplete_shown: false,
            autocomplete_items: Vec::new(),
            autocomplete_selected: 0,
            autocomplete_scroll_offset: 0,
            autocomplete_context: AutocompleteContext::FieldName,
        }
    }

    pub fn get_query(&self) -> &str {
        &self.input
    }

    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor_position, c);
        self.cursor_position += 1;
        self.reset_error_timer();
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.input.remove(self.cursor_position - 1);
            self.cursor_position -= 1;
            self.reset_error_timer();
        }
    }

    /// Context-aware word deletion for `flag=value` tokens:
    ///
    /// - Cursor **in value**  → delete only the value text, leaving `flag=` intact.
    /// - Cursor **in key**    → delete the whole `flag=value` token (including any
    ///   trailing space), but keep the space that precedes the token so adjacent
    ///   tokens stay correctly separated.
    /// - Cursor after a space → delete just the space (fallback to char delete).
    pub fn delete_word(&mut self) {
        if self.cursor_position == 0 {
            return;
        }

        let pos = self.cursor_position;

        // Bounds of the current token (space-delimited).
        let token_start = self.input[..pos]
            .rfind(' ')
            .map(|i| i + 1)
            .unwrap_or(0);

        if token_start == pos {
            // Cursor is right after a space with no token to its left.
            self.delete_char();
            return;
        }

        let token_end = self.input[pos..]
            .find(' ')
            .map(|i| pos + i)
            .unwrap_or(self.input.len());

        // Decide: are we in the key part or the value part?
        let token_before_cursor = &self.input[token_start..pos];

        if let Some(eq_pos) = token_before_cursor.find('=') {
            // ── Value case ────────────────────────────────────────────────────
            // Remove only the value text; leave `flag=` in place.
            let value_start = token_start + eq_pos + 1;
            if value_start < token_end {
                self.input.drain(value_start..token_end);
                self.cursor_position = value_start;
            } else {
                // Cursor is right after '=' with no value — delete the '=' sign.
                self.delete_char();
                return;
            }
        } else {
            // Cursor is in a word with no '=' anywhere in the token.
            // Check whether the full token contains '=' (cursor might be in the key
            // part of `flag=value` with the '=' to the right of the cursor).
            let full_token = &self.input[token_start..token_end];
            if full_token.contains('=') {
                // ── Key case ──────────────────────────────────────────────────
                // Cursor is in the key part (before '=').  Remove the entire
                // `flag=value` token.  Consume the trailing space if present so
                // no double-space is left behind, but leave the space that
                // precedes the token.
                let remove_end = if token_end < self.input.len()
                    && self.input.as_bytes()[token_end] == b' '
                {
                    token_end + 1
                } else {
                    token_end
                };
                self.input.drain(token_start..remove_end);
                self.cursor_position = token_start;
            } else {
                // ── Bare word (free value, no '=') ────────────────────────────
                // Standard char-by-char deletion.
                self.delete_char();
                return; // reset_error_timer already called by delete_char
            }
        }

        self.reset_error_timer();
    }

    pub fn autocomplete_next(&mut self) {
        if !self.autocomplete_items.is_empty() {
            self.autocomplete_selected =
                (self.autocomplete_selected + 1) % self.autocomplete_items.len();
            self.update_scroll_offset();
        }
    }

    pub fn autocomplete_prev(&mut self) {
        if !self.autocomplete_items.is_empty() {
            if self.autocomplete_selected == 0 {
                self.autocomplete_selected = self.autocomplete_items.len() - 1;
            } else {
                self.autocomplete_selected -= 1;
            }
            self.update_scroll_offset();
        }
    }

    pub fn accept_autocomplete(&mut self) -> Option<String> {
        if !self.autocomplete_items.is_empty()
            && self.autocomplete_selected < self.autocomplete_items.len()
        {
            let suggestion = self.autocomplete_items[self.autocomplete_selected].clone();

            // Find where to insert the suggestion
            let before_cursor = &self.input[..self.cursor_position];

            // Find the start of the current token
            let token_start = before_cursor
                .rfind(|c: char| c == ' ' || c == '=' || c == ',')
                .map(|i| i + 1)
                .unwrap_or(0);

            // Detect context to see if we're completing a field name
            let context = detect_context(&self.input, self.cursor_position);
            let is_field_name = matches!(context, AutocompleteContext::FieldName);

            // Wrap in quotes if the suggestion contains spaces
            let mut suggestion_to_insert = if suggestion.contains(' ') {
                format!("\"{}\"", suggestion)
            } else {
                suggestion.clone()
            };

            // Add '=' after field names
            if is_field_name {
                suggestion_to_insert.push('=');
            }

            // Replace the current token with the suggestion
            self.input
                .replace_range(token_start..self.cursor_position, &suggestion_to_insert);
            self.cursor_position = token_start + suggestion_to_insert.len();

            self.autocomplete_shown = false;
            self.autocomplete_items.clear();
            self.reset_error_timer();

            Some(suggestion)
        } else {
            None
        }
    }

    pub fn update_autocomplete(&mut self, suggestions: Vec<String>) {
        self.autocomplete_items = suggestions;
        self.autocomplete_selected = 0;
        self.autocomplete_scroll_offset = 0;
        // Detect and store current context
        self.autocomplete_context = detect_context(&self.input, self.cursor_position);
        // Show autocomplete automatically for discoverability
        self.autocomplete_shown = !self.autocomplete_items.is_empty();
    }

    #[allow(dead_code)]
    pub fn validate(&mut self) {
        // Check if enough time has passed (300ms debounce)
        if let Some(timer) = self.error_debounce_timer {
            if timer.elapsed().as_millis() < 300 {
                return;
            }
        }

        // Parse and check for errors
        match parse_query(&self.input) {
            Ok(_) => {
                self.parse_error = None;
            }
            Err(e) => {
                self.parse_error = Some(e);
            }
        }
    }

    fn reset_error_timer(&mut self) {
        self.error_debounce_timer = Some(Instant::now());
        self.parse_error = None;
    }

    fn update_scroll_offset(&mut self) {
        let visible_items = 10;
        if self.autocomplete_selected < self.autocomplete_scroll_offset {
            self.autocomplete_scroll_offset = self.autocomplete_selected;
        } else if self.autocomplete_selected >= self.autocomplete_scroll_offset + visible_items {
            self.autocomplete_scroll_offset = self.autocomplete_selected - visible_items + 1;
        }
    }

    fn get_icon_for_field(&self, field_name: &str) -> &str {
        match field_name {
            "name" => "> ",
            "country" => "@ ",
            "countrycode" => "# ",
            "state" => ". ",
            "language" => "~ ",
            "tag" => "* ",
            "codec" => "% ",
            "bitrate_min" | "bitrate_max" => "= ",
            "order" => "^ ",
            "reverse" => "< ",
            "hidebroken" => "! ",
            "is_https" => "$ ",
            "page" => "& ",
            _ => "? ",
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        // Calculate popup size
        let popup_width = 52;
        let base_height = 5; // Input block (3) + padding (2)

        // Show all autocomplete items (up to 14 for field names)
        let max_autocomplete_items = 14;
        let autocomplete_height = if self.autocomplete_shown {
            (self.autocomplete_items.len().min(max_autocomplete_items) + 2) as u16
        // +2 for borders
        } else {
            0
        };
        let error_height = if self.parse_error.is_some() { 2 } else { 0 };
        let popup_height = base_height + autocomplete_height + error_height;

        // Center popup based on MAXIMUM possible height to keep position stable
        // Maximum height = base + max autocomplete + max error
        let max_popup_height = base_height + (max_autocomplete_items as u16 + 2) + 2;
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area
            .height
            .saturating_sub(max_popup_height.min(area.height)))
            / 2;

        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height.min(area.height.saturating_sub(popup_y)),
        };

        // Clear the area
        f.render_widget(Clear, popup_area);

        // Split into sections
        let mut constraints = vec![
            Constraint::Length(3), // Title + input
        ];

        if self.autocomplete_shown {
            constraints.push(Constraint::Length(autocomplete_height));
        }

        if self.parse_error.is_some() {
            constraints.push(Constraint::Length(error_height));
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(popup_area);

        let mut current_chunk = 0;

        // Render input
        self.render_input(f, chunks[current_chunk], theme);
        current_chunk += 1;

        // Render autocomplete if shown
        if self.autocomplete_shown {
            self.render_autocomplete(f, chunks[current_chunk], theme);
            current_chunk += 1;
        }

        // Render error if present
        if self.parse_error.is_some() {
            self.render_error(f, chunks[current_chunk]);
            current_chunk += 1;
        }

        let _ = current_chunk;
    }

    fn render_input(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        // Highlight syntax in input
        let highlighted = self.highlight_syntax(theme);

        let input_paragraph = Paragraph::new(highlighted).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_popup)
                .title("Search Stations")
                .title_alignment(Alignment::Center)
                .padding(Padding::horizontal(1)),
        );

        f.render_widget(input_paragraph, area);

        // Render cursor — offset by border (1) + horizontal padding (1) = 2
        if self.cursor_position <= self.input.len() {
            let cursor_x = area.x + 2 + self.cursor_position as u16;
            let cursor_y = area.y + 1;

            if cursor_x < area.x + area.width - 2 {
                f.set_cursor_position((cursor_x, cursor_y));
            }
        }
    }

    fn render_autocomplete(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let visible_items = 14; // Show all items up to 14
        let items: Vec<ListItem> = self
            .autocomplete_items
            .iter()
            .skip(self.autocomplete_scroll_offset)
            .take(visible_items)
            .enumerate()
            .map(|(i, item)| {
                let index = i + self.autocomplete_scroll_offset;
                let style = if index == self.autocomplete_selected {
                    theme.selection
                } else {
                    Style::default()
                };
                // Only add icon if we're showing field names, not field values
                let display_text = match &self.autocomplete_context {
                    AutocompleteContext::FieldName => {
                        let icon = self.get_icon_for_field(item);
                        format!("{}{}", icon, item)
                    }
                    AutocompleteContext::FieldValue(_) => item.to_string(),
                    AutocompleteContext::InvalidComma => item.to_string(),
                };
                ListItem::new(display_text).style(style)
            })
            .collect();

        let total = self.autocomplete_items.len();
        let title = if total > 0 {
            format!(" Showing {} of {} ", items.len().min(visible_items), total)
        } else {
            " No suggestions ".to_string()
        };

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_popup)
                .title(title)
                .padding(Padding::horizontal(1)),
        );

        f.render_widget(list, area);
    }

    fn render_error(&self, f: &mut Frame, area: Rect) {
        if let Some(error) = &self.parse_error {
            let error_text = format!("{}", error);
            let paragraph =
                Paragraph::new(error_text).style(Style::default().fg(ratatui::style::Color::Red));
            f.render_widget(paragraph, area);
        }
    }

    fn highlight_syntax(&self, theme: &Theme) -> Line<'_> {
        // Parse the input and highlight fields
        let mut spans = Vec::new();
        let mut current_pos = 0;

        // Split by spaces to get field=value pairs
        for part in self.input.split_whitespace() {
            // Find the position of this part in the original string
            if let Some(pos) = self.input[current_pos..].find(part) {
                let actual_pos = current_pos + pos;

                // Add any spaces before this part
                if actual_pos > current_pos {
                    spans.push(Span::raw(&self.input[current_pos..actual_pos]));
                }

                // Check if this is a field=value pair
                if let Some(equals_pos) = part.find('=') {
                    let field = &part[..equals_pos];
                    let value_with_equals = &part[equals_pos..];

                    // Check if field is valid
                    let field_style = if parser::validate_field(&field.to_lowercase()) {
                        theme.tab_active
                    } else {
                        Style::default().fg(ratatui::style::Color::Red)
                    };

                    spans.push(Span::styled(field.to_string(), field_style));

                    // Check if this is a country or language field with commas
                    let field_lower = field.to_lowercase();
                    if (field_lower == "country" || field_lower == "language")
                        && value_with_equals.contains(',')
                    {
                        // Highlight commas in red for country/language fields
                        for ch in value_with_equals.chars() {
                            if ch == ',' {
                                spans.push(Span::styled(
                                    ch.to_string(),
                                    Style::default().fg(ratatui::style::Color::Red),
                                ));
                            } else {
                                spans.push(Span::raw(ch.to_string()));
                            }
                        }
                    } else {
                        // Normal value rendering
                        spans.push(Span::raw(value_with_equals.to_string()));
                    }
                } else {
                    // Bare word: treated as an implicit name filter — use section_header color
                    spans.push(Span::styled(part.to_string(), theme.section_header));
                }

                current_pos = actual_pos + part.len();
            }
        }

        // Add any remaining text
        if current_pos < self.input.len() {
            spans.push(Span::raw(&self.input[current_pos..]));
        }

        // If input is empty, show a dim cursor
        if spans.is_empty() {
            spans.push(Span::raw(""));
        }

        Line::from(spans)
    }
}
