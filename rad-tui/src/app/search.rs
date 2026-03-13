use anyhow::Result;
use tui_kit::LogLevel;
use rad_core::search::{format_query, get_suggestions, is_default_query};

use super::App;

impl App {
    pub fn open_search_popup(&mut self) {
        let initial_query = if is_default_query(&self.current_query) {
            String::new()
        } else {
            format_query(&self.current_query)
        };

        let mut popup = crate::ui::SearchPopup::new(initial_query.clone());

        let suggestions = get_suggestions(&initial_query, popup.cursor_position, &self.autocomplete_data);
        popup.update_autocomplete(suggestions);

        self.search_popup = Some(popup);
    }

    pub fn close_search_popup(&mut self) {
        self.search_popup = None;
    }

    pub async fn execute_search(&mut self) -> Result<()> {
        tracing::info!("execute_search: Starting with query: {:?}", self.current_query);

        let mut query = self.current_query.clone();
        query.reset_pagination();

        tracing::info!("execute_search: Pagination reset");

        self.pages_cache.clear();
        self.current_page = 1;
        self.is_last_page = false;
        self.highest_page_loaded = 0;

        tracing::info!("execute_search: Cache cleared, calling API");

        self.loading = true;
        match self.api_client.advanced_search(&query).await {
            Ok(stations) => {
                tracing::info!("execute_search: API returned {} stations", stations.len());
                tracing::info!("execute_search: query.limit = {}", query.limit);
                self.is_last_page = stations.len() < query.limit;
                tracing::info!("execute_search: is_last_page = {}", self.is_last_page);

                if !stations.is_empty() {
                    self.highest_page_loaded = 1;
                }

                self.pages_cache.insert(1, stations.clone());

                self.stations = stations;
                self.browse_stations = self.stations.clone();
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.loading = false;

                tracing::info!("execute_search: Stations loaded, updating UI");

                if !is_default_query(&query) {
                    let query_str = format_query(&query);
                    if let Err(e) = self.search_history.add_query(query_str, Some(self.stations.len())) {
                        tracing::warn!("Failed to save search history: {}", e);
                    }
                }

                let msg = format!("Found {} stations (is_last_page={})", self.stations.len(), self.is_last_page);
                self.status_message = Some(msg.clone());
                self.add_log(LogLevel::Info, msg);

                tracing::info!("execute_search: Log added");

                if self.search_popup.is_some() {
                    self.close_search_popup();
                }

                tracing::info!("execute_search: Completed successfully, highest_page={}", self.highest_page_loaded);
                Ok(())
            }
            Err(e) => {
                tracing::error!("execute_search: API call failed: {}", e);
                self.loading = false;
                self.show_error(format!("Search failed: {}", e));
                Err(e)
            }
        }
    }

    pub async fn load_page(&mut self, page: usize) -> Result<()> {
        if page == 0 {
            return Ok(());
        }

        tracing::info!("load_page: Loading page {}, current_page={}, is_last_page={}", page, self.current_page, self.is_last_page);

        if let Some(stations) = self.pages_cache.get(&page) {
            tracing::info!("load_page: Found page {} in cache with {} stations", page, stations.len());
            self.stations = stations.clone();
            self.current_page = page;
            self.selected_index = 0;
            self.scroll_offset = 0;

            let query = &self.current_query;
            if stations.len() < query.limit {
                self.is_last_page = true;
            } else if self.pages_cache.contains_key(&(page + 1)) {
                self.is_last_page = false;
            } else if page >= self.highest_page_loaded {
                self.is_last_page = false;
            }

            let msg = format!("Loaded page {} from cache ({} stations)", page, stations.len());
            self.status_message = Some(msg.clone());
            self.add_log(LogLevel::Debug, msg);

            tracing::info!("load_page: Cache loaded, is_last_page={}", self.is_last_page);

            return Ok(());
        }

        let mut query = self.current_query.clone();
        query.offset = (page - 1) * query.limit;

        tracing::info!("load_page: Page {} not in cache, fetching from API with offset={}", page, query.offset);

        self.loading = true;
        match self.api_client.advanced_search(&query).await {
            Ok(stations) => {
                tracing::info!("load_page: API returned {} stations for page {}", stations.len(), page);
                self.is_last_page = stations.len() < query.limit;

                if page > self.highest_page_loaded && !stations.is_empty() {
                    self.highest_page_loaded = page;
                    tracing::info!("load_page: Updated highest_page_loaded to {}", self.highest_page_loaded);
                }

                if self.pages_cache.len() >= 5 {
                    let current = self.current_page;
                    let to_remove = self.pages_cache.keys()
                        .map(|k| (*k, (*k as i32 - current as i32).abs()))
                        .max_by_key(|(_, dist)| *dist)
                        .map(|(k, _)| k);

                    if let Some(key) = to_remove {
                        self.pages_cache.remove(&key);
                    }
                }

                self.pages_cache.insert(page, stations.clone());

                self.stations = stations;
                self.current_page = page;
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.loading = false;

                let msg = format!("Loaded page {} ({} stations)", page, self.stations.len());
                self.status_message = Some(msg.clone());
                self.add_log(LogLevel::Info, msg);

                tracing::info!("load_page: Success, is_last_page={}, highest_page={}", self.is_last_page, self.highest_page_loaded);

                Ok(())
            }
            Err(e) => {
                self.loading = false;
                self.show_error(format!("Failed to load page: {}", e));
                Err(e)
            }
        }
    }

    pub async fn next_page(&mut self) -> Result<()> {
        tracing::info!("next_page: called, is_last_page={}, current_page={}", self.is_last_page, self.current_page);
        if !self.is_last_page {
            tracing::info!("next_page: calling load_page({})", self.current_page + 1);
            self.load_page(self.current_page + 1).await
        } else {
            tracing::info!("next_page: blocked because is_last_page=true");
            self.add_log(LogLevel::Warning, "Cannot go to next page: already on last page".to_string());
            Ok(())
        }
    }

    pub async fn prev_page(&mut self) -> Result<()> {
        if self.current_page > 1 {
            self.load_page(self.current_page - 1).await
        } else {
            Ok(())
        }
    }

    pub async fn first_page(&mut self) -> Result<()> {
        if self.current_page != 1 {
            self.load_page(1).await
        } else {
            Ok(())
        }
    }
}
