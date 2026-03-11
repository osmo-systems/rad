use anyhow::{Context, Result};
use rand::seq::SliceRandom;
use reqwest::Client;
use std::net::IpAddr;
use std::time::Duration;
use tracing::{debug, info, warn};
use trust_dns_resolver::config::*;
use trust_dns_resolver::TokioAsyncResolver;

use super::models::{ClickResponse, Country, Language, Station, Tag, VoteResponse};

const API_DNS_NAME: &str = "all.api.radio-browser.info";
const USER_AGENT: &str = "radm/0.1.0";

#[derive(Clone)]
pub struct RadioBrowserClient {
    client: Client,
    base_urls: Vec<String>,
    current_url_index: usize,
}

impl RadioBrowserClient {
    pub async fn new() -> Result<Self> {
        info!("Initializing Radio Browser API client");
        
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        let base_urls = Self::discover_servers().await?;
        
        if base_urls.is_empty() {
            anyhow::bail!("No Radio Browser servers found");
        }

        info!("Found {} Radio Browser servers", base_urls.len());

        Ok(Self {
            client,
            base_urls,
            current_url_index: 0,
        })
    }

    async fn discover_servers() -> Result<Vec<String>> {
        debug!("Discovering Radio Browser servers via DNS");
        
        let resolver = Self::create_resolver().await?;

        let response = resolver
            .lookup_ip(API_DNS_NAME)
            .await
            .context("Failed to resolve Radio Browser DNS")?;

        let mut ips: Vec<IpAddr> = response.iter().collect();
        
        // Randomize server list for load balancing
        let mut rng = rand::thread_rng();
        ips.shuffle(&mut rng);

        let mut base_urls = Vec::new();
        for ip in ips {
            // Perform reverse DNS lookup to get hostname
            match resolver.reverse_lookup(ip).await {
                Ok(names) => {
                    if let Some(name) = names.iter().next() {
                        let hostname = name.to_string();
                        let url = format!("https://{}", hostname.trim_end_matches('.'));
                        base_urls.push(url);
                        debug!("Found server: {}", hostname);
                    }
                }
                Err(e) => {
                    warn!("Failed reverse DNS lookup for {}: {}", ip, e);
                    // Fallback to IP address
                    base_urls.push(format!("https://{}", ip));
                }
            }
        }

        Ok(base_urls)
    }

    async fn create_resolver() -> Result<TokioAsyncResolver> {
        #[cfg(target_os = "macos")]
        {
            Self::create_resolver_macos()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(TokioAsyncResolver::tokio(
                ResolverConfig::default(),
                ResolverOpts::default(),
            ))
        }
    }

    #[cfg(target_os = "macos")]
    fn create_resolver_macos() -> Result<TokioAsyncResolver> {
        debug!("Creating DNS resolver for macOS with explicit DNS servers");

        // On macOS, /etc/resolv.conf is ignored and system DNS config cannot be reliably read
        // Use Google Public DNS (or Cloudflare as fallback) instead
        let config = ResolverConfig::google();

        debug!("Using Google Public DNS (8.8.8.8, 8.8.4.4) for macOS DNS resolution");
        Ok(TokioAsyncResolver::tokio(config, ResolverOpts::default()))
    }

    fn get_base_url(&self) -> &str {
        &self.base_urls[self.current_url_index]
    }

    fn rotate_server(&mut self) {
        self.current_url_index = (self.current_url_index + 1) % self.base_urls.len();
        debug!("Rotated to server: {}", self.get_base_url());
    }

    async fn get<T: serde::de::DeserializeOwned>(&mut self, endpoint: &str) -> Result<T> {
        let url = format!("{}{}", self.get_base_url(), endpoint);
        debug!("GET request to: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            warn!("Request failed with status: {}, rotating server", response.status());
            self.rotate_server();
            anyhow::bail!("Request failed with status: {}", response.status());
        }

        let data = response
            .json::<T>()
            .await
            .context("Failed to parse JSON response")?;

        Ok(data)
    }

    async fn post<T: serde::de::DeserializeOwned>(&mut self, endpoint: &str) -> Result<T> {
        let url = format!("{}{}", self.get_base_url(), endpoint);
        debug!("POST request to: {}", url);

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            warn!("Request failed with status: {}, rotating server", response.status());
            self.rotate_server();
            anyhow::bail!("Request failed with status: {}", response.status());
        }

        let data = response
            .json::<T>()
            .await
            .context("Failed to parse JSON response")?;

        Ok(data)
    }

    // Search stations by name
    pub async fn search_stations(&mut self, query: &str, limit: usize) -> Result<Vec<Station>> {
        let endpoint = format!(
            "/json/stations/search?name={}&limit={}&hidebroken=true&order=votes&reverse=true",
            urlencoding::encode(query),
            limit
        );
        self.get(&endpoint).await
    }

    // Get popular stations
    pub async fn get_popular_stations(&mut self, limit: usize) -> Result<Vec<Station>> {
        let endpoint = format!(
            "/json/stations?limit={}&hidebroken=true&order=votes&reverse=true",
            limit
        );
        self.get(&endpoint).await
    }

    // Get stations by country
    pub async fn get_stations_by_country(&mut self, country: &str, limit: usize) -> Result<Vec<Station>> {
        let endpoint = format!(
            "/json/stations/bycountry/{}?limit={}&hidebroken=true&order=votes&reverse=true",
            urlencoding::encode(country),
            limit
        );
        self.get(&endpoint).await
    }

    // Get stations by tag
    pub async fn get_stations_by_tag(&mut self, tag: &str, limit: usize) -> Result<Vec<Station>> {
        let endpoint = format!(
            "/json/stations/bytag/{}?limit={}&hidebroken=true&order=votes&reverse=true",
            urlencoding::encode(tag),
            limit
        );
        self.get(&endpoint).await
    }

    // Get stations by language
    pub async fn get_stations_by_language(&mut self, language: &str, limit: usize) -> Result<Vec<Station>> {
        let endpoint = format!(
            "/json/stations/bylanguage/{}?limit={}&hidebroken=true&order=votes&reverse=true",
            urlencoding::encode(language),
            limit
        );
        self.get(&endpoint).await
    }

    // Get list of countries
    pub async fn get_countries(&mut self) -> Result<Vec<Country>> {
        let endpoint = "/json/countries?order=stationcount&reverse=true";
        self.get(endpoint).await
    }

    // Get list of tags
    pub async fn get_tags(&mut self, limit: usize) -> Result<Vec<Tag>> {
        let endpoint = format!("/json/tags?limit={}&order=stationcount&reverse=true", limit);
        self.get(&endpoint).await
    }

    // Get list of languages
    pub async fn get_languages(&mut self) -> Result<Vec<Language>> {
        let endpoint = "/json/languages?order=stationcount&reverse=true";
        self.get(endpoint).await
    }

    // Count station click (tracks when user plays a station)
    pub async fn count_click(&mut self, station_uuid: &str) -> Result<ClickResponse> {
        let endpoint = format!("/json/url/{}", station_uuid);
        self.get(&endpoint).await
    }

    // Vote for a station
    pub async fn vote_for_station(&mut self, station_uuid: &str) -> Result<VoteResponse> {
        let endpoint = format!("/json/vote/{}", station_uuid);
        self.get(&endpoint).await
    }

    // Advanced search with multiple parameters
    pub async fn advanced_search(
        &mut self,
        query: &crate::search::SearchQuery,
    ) -> Result<Vec<Station>> {
        let mut params = Vec::new();

        // Add search parameters
        if let Some(name) = &query.name {
            params.push(format!("name={}", urlencoding::encode(name)));
        }
        if let Some(country) = &query.country {
            params.push(format!("country={}", urlencoding::encode(country)));
        }
        if let Some(countrycode) = &query.countrycode {
            params.push(format!("countrycode={}", urlencoding::encode(countrycode)));
        }
        if let Some(state) = &query.state {
            params.push(format!("state={}", urlencoding::encode(state)));
        }
        if let Some(language) = &query.language {
            params.push(format!("language={}", urlencoding::encode(language)));
        }
        if let Some(tags) = &query.tags {
            // tagList = AND logic (all tags must match)
            // Encode each tag separately, then join with comma (comma must NOT be encoded)
            let tag_str = tags.iter()
                .map(|t| urlencoding::encode(t).to_string())
                .collect::<Vec<_>>()
                .join(",");
            params.push(format!("tagList={}", tag_str));
        }
        if let Some(codec) = &query.codec {
            params.push(format!("codec={}", urlencoding::encode(codec)));
        }
        if let Some(bitrate_min) = query.bitrate_min {
            params.push(format!("bitrateMin={}", bitrate_min));
        }
        if let Some(bitrate_max) = query.bitrate_max {
            params.push(format!("bitrateMax={}", bitrate_max));
        }
        if let Some(order) = &query.order {
            params.push(format!("order={}", order));
        }
        if let Some(reverse) = query.reverse {
            params.push(format!("reverse={}", reverse));
        }
        if let Some(hidebroken) = query.hidebroken {
            params.push(format!("hidebroken={}", hidebroken));
        }
        if let Some(is_https) = query.is_https {
            params.push(format!("is_https={}", is_https));
        }

        // Add pagination parameters
        params.push(format!("limit={}", query.limit));
        params.push(format!("offset={}", query.offset));

        let endpoint = format!("/json/stations/search?{}", params.join("&"));
        tracing::info!("advanced_search: API endpoint: {}", endpoint);
        self.get(&endpoint).await
    }
}
