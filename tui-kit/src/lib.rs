//! `tui-kit` — reusable TUI theme, widget frames, and layout helpers.
//!
//! Built on top of [ratatui](https://github.com/ratatui-org/ratatui).
//! Designed to be shared across multiple terminal-UI projects that follow
//! the same visual design language.
//!
//! ## Modules
//!
//! | Module   | Contents |
//! |----------|----------|
//! | [`theme`] | [`Theme`] struct — the full color/style palette |
//! | [`block`] | [`block::panel_block`], [`block::popup_block`], [`block::widget_title`] |
//! | [`tabs`]  | [`tabs::tab_line`] — horizontal tab-bar line for block titles |
//! | [`popup`] | [`popup::centered_popup`] — centered overlay area + Clear |

pub mod block;
pub mod popup;
pub mod tabs;
pub mod theme;
pub mod toast;

pub use theme::Theme;
pub use toast::{Toast, ToastLevel, render_toasts};
