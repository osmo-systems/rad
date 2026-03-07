// Pagination logic with LRU caching
// TODO: Implement in Step 7

use std::collections::HashMap;
use crate::api::models::Station;

pub struct PageCache {
    pages: HashMap<usize, Vec<Station>>,
    lru_order: Vec<usize>,
    max_pages: usize,
}

impl PageCache {
    pub fn new(max_pages: usize) -> Self {
        Self {
            pages: HashMap::new(),
            lru_order: Vec::new(),
            max_pages,
        }
    }
    
    pub fn get(&mut self, page: usize) -> Option<&Vec<Station>> {
        if self.pages.contains_key(&page) {
            // Update LRU order
            self.lru_order.retain(|&p| p != page);
            self.lru_order.push(page);
            self.pages.get(&page)
        } else {
            None
        }
    }
    
    pub fn insert(&mut self, page: usize, stations: Vec<Station>) {
        // Evict oldest page if cache is full
        if self.pages.len() >= self.max_pages && !self.pages.contains_key(&page) {
            if let Some(&oldest_page) = self.lru_order.first() {
                self.pages.remove(&oldest_page);
                self.lru_order.remove(0);
            }
        }
        
        // Update LRU order
        self.lru_order.retain(|&p| p != page);
        self.lru_order.push(page);
        
        // Insert page
        self.pages.insert(page, stations);
    }
    
    pub fn clear(&mut self) {
        self.pages.clear();
        self.lru_order.clear();
    }
}
