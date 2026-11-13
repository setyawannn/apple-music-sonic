use super::auth::AuthManager;
use super::models::{CatalogCharts, CatalogSearchResult, CloudPlaylist, SearchSuggestions, StoreAlbum, StoreArtist, StoreSong};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

pub struct ApiClient {
    http: reqwest::Client,
    pub auth: Arc<AuthManager>,
    base_url: String,
    edge_url: String,
}

impl ApiClient {
    pub fn new(auth: Arc<AuthManager>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            http,
            auth,
            base_url: "https://amp-api.music.apple.com".to_string(),
            edge_url: "https://amp-api-edge.music.apple.com".to_string(),
        }
    }

    pub fn with_base_url(auth: Arc<AuthManager>, base_url: String, edge_url: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        Self {
            http,
            auth,
            base_url,
            edge_url,
        }
    }

    fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        let dev_token = self.auth.developer_token();
        if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", dev_token)) {
            headers.insert(AUTHORIZATION, val);
        }

        if let Some(user_token) = self.auth.user_token() {
            if let Ok(val) = HeaderValue::from_str(&user_token) {
                headers.insert("Music-User-Token", val);
            }
        }

        headers.insert("Origin", HeaderValue::from_static("https://music.apple.com"));
        headers.insert("Referer", HeaderValue::from_static("https://music.apple.com/"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        headers
    }

    pub async fn search_catalog(
        &self,
        term: &str,
        types: Option<&str>,
        limit: u32,
    ) -> anyhow::Result<CatalogSearchResult> {
        let trimmed = term.trim();
        if trimmed.is_empty() {
            return Ok(CatalogSearchResult::default());
        }

        let storefront = self.auth.storefront();
        let language = self.auth.language();
        let types_param = types.unwrap_or("songs,albums,artists");

        let url = format!("{}/v1/catalog/{}/search", self.base_url, storefront);

        let limit_str = limit.clamp(1, 50).to_string();
        let query_params = [
            ("term", trimmed),
            ("types", types_param),
            ("limit", &limit_str),
            ("l", &language),
            ("platform", "desktop"),
        ];

        let response = self
            .http
            .get(&url)
            .headers(self.build_headers())
            .query(&query_params)
            .send()
            .await?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("401 Unauthorized: Session token expired or invalid");
        } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            anyhow::bail!("429 Too Many Requests: Rate limited by Apple Music");
        } else if !status.is_success() {
            anyhow::bail!("Apple Music API error: HTTP {}", status);
        }

        let json: Value = response.json().await?;
        Ok(parse_catalog_search_json(&json))
    }

    pub async fn get_search_suggestions(&self, term: &str) -> anyhow::Result<SearchSuggestions> {
        let trimmed = term.trim();
        if trimmed.is_empty() {
            return Ok(SearchSuggestions::default());
        }

        let storefront = self.auth.storefront();
        let language = self.auth.language();

        let url = format!("{}/v1/catalog/{}/search/suggestions", self.edge_url, storefront);

        let query_params = [
            ("term", trimmed),
            ("kinds", "terms"),
            ("l", &language),
        ];

        let response = self
            .http
            .get(&url)
            .headers(self.build_headers())
            .query(&query_params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(SearchSuggestions::default());
        }

        let json: Value = response.json().await?;
        let mut terms = Vec::new();

        if let Some(terms_arr) = json["results"]["suggestions"].as_array() {
            for item in terms_arr {
                if let Some(display_term) = item["searchTerm"].as_str().or_else(|| item["displayTerm"].as_str()) {
                    terms.push(display_term.to_string());
                }
            }
        }

        Ok(SearchSuggestions { terms })
    }

    pub async fn get_catalog_album(&self, id: &str) -> anyhow::Result<StoreAlbum> {
        let storefront = self.auth.storefront();
        let language = self.auth.language();

        let url = format!(
            "{}/v1/catalog/{}/albums/{}?l={}&include=tracks",
            self.base_url, storefront, id, language
        );

        let response = self
            .http
            .get(&url)
            .headers(self.build_headers())
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!("Album not found (404)");
        } else if !response.status().is_success() {
            anyhow::bail!("Failed to get album: HTTP {}", response.status());
        }

        let json: Value = response.json().await?;
        let data = json["data"]
            .as_array()
            .and_then(|a| a.first())
            .ok_or_else(|| anyhow::anyhow!("Missing album data in response"))?;

        Ok(parse_single_album_json(data))
    }

    pub async fn get_charts(&self, types: Option<&str>, limit: u32) -> anyhow::Result<CatalogCharts> {
        let storefront = self.auth.storefront();
        let language = self.auth.language();
        let types_param = types.unwrap_or("songs,albums");

        let url = format!(
            "{}/v1/catalog/{}/charts?types={}&limit={}&l={}",
            self.base_url,
            storefront,
            types_param,
            limit.clamp(1, 50),
            language
        );

        let response = self
            .http
            .get(&url)
            .headers(self.build_headers())
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to get charts: HTTP {}", response.status());
        }

        let json: Value = response.json().await?;
        let mut charts = CatalogCharts::default();

        if let Some(song_charts) = json["results"]["songs"].as_array() {
            if let Some(first_chart) = song_charts.first() {
                if let Some(songs_data) = first_chart["data"].as_array() {
                    for s in songs_data {
                        charts.songs.push(parse_single_song_json(s));
                    }
                }
            }
        }

        if let Some(album_charts) = json["results"]["albums"].as_array() {
            if let Some(first_chart) = album_charts.first() {
                if let Some(albums_data) = first_chart["data"].as_array() {
                    for a in albums_data {
                        charts.albums.push(parse_single_album_json(a));
                    }
                }
            }
        }

        Ok(charts)
    }

    pub async fn get_personal_library_songs(&self) -> anyhow::Result<Vec<StoreSong>> {
        if !self.auth.is_authenticated() {
            anyhow::bail!("User is not authenticated with Apple ID");
        }

        let language = self.auth.language();
        let url = format!("{}/v1/me/library/songs?limit=100&l={}", self.base_url, language);

        let response = self
            .http
            .get(&url)
            .headers(self.build_headers())
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to get personal library: HTTP {}", response.status());
        }

        let json: Value = response.json().await?;
        let mut songs = Vec::new();

        if let Some(data) = json["data"].as_array() {
            for item in data {
                songs.push(parse_single_song_json(item));
            }
        }

        Ok(songs)
    }

    pub async fn get_personal_recommendations(&self) -> anyhow::Result<Vec<StoreAlbum>> {
        if !self.auth.is_authenticated() {
            anyhow::bail!("User is not authenticated with Apple ID");
        }

        let language = self.auth.language();
        let url = format!("{}/v1/me/recommendations?l={}", self.base_url, language);

        let response = self
            .http
            .get(&url)
            .headers(self.build_headers())
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to get recommendations: HTTP {}", response.status());
        }

        let json: Value = response.json().await?;
        let mut albums = Vec::new();

        if let Some(data) = json["data"].as_array() {
            for rec in data {
                if let Some(items) = rec["relationships"]["contents"]["data"].as_array() {
                    for item in items {
                        if item["type"].as_str() == Some("albums") {
                            albums.push(parse_single_album_json(item));
                        }
                    }
                }
            }
        }

        Ok(albums)
    }

    pub async fn get_user_playlists(&self) -> anyhow::Result<Vec<CloudPlaylist>> {
        if !self.auth.is_authenticated() {
            anyhow::bail!("User is not authenticated with Apple ID");
        }

        let language = self.auth.language();
        // No `include=tracks` here: Apple returns **400** for it on library playlists
        // (measured; `include=trackCount` and `extend=tracks` are accepted but add nothing).
        // Track counts are therefore absent from the list — tracks are fetched on demand by
        // `get_playlist_tracks` when a playlist is opened.
        let url = format!("{}/v1/me/library/playlists?limit=100&l={}", self.base_url, language);

        let response = self
            .http
            .get(&url)
            .headers(self.build_headers())
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to get user playlists: HTTP {}", response.status());
        }

        let json: Value = response.json().await?;
        let mut playlists = Vec::new();

        if let Some(data) = json["data"].as_array() {
            for item in data {
                playlists.push(parse_single_playlist_json(item));
            }
        }

        Ok(playlists)
    }

    pub async fn get_playlist_tracks(&self, playlist_id: &str) -> anyhow::Result<Vec<StoreSong>> {
        if !self.auth.is_authenticated() {
            anyhow::bail!("User is not authenticated with Apple ID");
        }

        let language = self.auth.language();
        let url = format!("{}/v1/me/library/playlists/{}/tracks?limit=100&l={}", self.base_url, playlist_id, language);

        let response = self
            .http
            .get(&url)
            .headers(self.build_headers())
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to get playlist tracks: HTTP {}", response.status());
        }

        let json: Value = response.json().await?;
        let mut songs = Vec::new();

        if let Some(data) = json["data"].as_array() {
            for item in data {
                songs.push(parse_single_song_json(item));
            }
        }

        Ok(songs)
    }

    /// Create a library (iCloud) playlist owned by the user. Returns its id.
    ///
    /// Measured contract: `POST /v1/me/library/playlists` with a bare
    /// `{ "attributes": { "name" } }` body returns **201** and the new id under
    /// `data[0].id`. Tracks are added separately (see `add_tracks_to_library_playlist`).
    pub async fn create_library_playlist(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> anyhow::Result<String> {
        if !self.auth.is_authenticated() {
            anyhow::bail!("User is not authenticated with Apple ID");
        }
        let language = self.auth.language();
        let url = format!("{}/v1/me/library/playlists?l={}", self.base_url, language);

        let mut attrs = serde_json::json!({ "name": name });
        if let Some(d) = description {
            if !d.is_empty() {
                attrs["description"] = Value::String(d.to_string());
            }
        }
        let body = serde_json::json!({ "attributes": attrs });

        let response = self
            .http
            .post(&url)
            .headers(self.build_headers())
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let json: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            anyhow::bail!(
                "Create playlist failed: HTTP {} ({})",
                status,
                apple_error(&json, "no detail")
            );
        }
        json["data"][0]["id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Create playlist: response contained no id"))
    }

    /// Rename a library playlist.
    ///
    /// Deliberately **no `?l=` query parameter**: adding one makes Apple return
    /// 500 (measured during the write probe). A bare `{ "attributes": { "name" } }`
    /// body returns 204.
    pub async fn rename_library_playlist(&self, playlist_id: &str, name: &str) -> anyhow::Result<()> {
        if !self.auth.is_authenticated() {
            anyhow::bail!("User is not authenticated with Apple ID");
        }
        let url = format!("{}/v1/me/library/playlists/{}", self.base_url, playlist_id);
        let body = serde_json::json!({ "attributes": { "name": name } });

        let response = self
            .http
            .put(&url)
            .headers(self.build_headers())
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let json: Value = response.json().await.unwrap_or(Value::Null);
            anyhow::bail!(
                "Rename playlist failed: HTTP {} ({})",
                status,
                apple_error(&json, "no detail")
            );
        }
        Ok(())
    }

    /// Delete a library playlist.
    pub async fn delete_library_playlist(&self, playlist_id: &str) -> anyhow::Result<()> {
        if !self.auth.is_authenticated() {
            anyhow::bail!("User is not authenticated with Apple ID");
        }
        let url = format!("{}/v1/me/library/playlists/{}", self.base_url, playlist_id);
        let response = self.http.delete(&url).headers(self.build_headers()).send().await?;
        let status = response.status();
        if !status.is_success() {
            let json: Value = response.json().await.unwrap_or(Value::Null);
            anyhow::bail!(
                "Delete playlist failed: HTTP {} ({})",
                status,
                apple_error(&json, "no detail")
            );
        }
        Ok(())
    }

    /// Add tracks to a library playlist.
    ///
    /// Catalog songs are referenced as `type: "songs"` (numeric id); the user's
    /// own library songs as `type: "library-songs"` (id prefixed `i.`). Both were
    /// verified to return 204.
    pub async fn add_tracks_to_library_playlist(
        &self,
        playlist_id: &str,
        track_ids: &[String],
    ) -> anyhow::Result<()> {
        if !self.auth.is_authenticated() {
            anyhow::bail!("User is not authenticated with Apple ID");
        }
        if track_ids.is_empty() {
            return Ok(());
        }
        let language = self.auth.language();
        let url = format!(
            "{}/v1/me/library/playlists/{}/tracks?l={}",
            self.base_url, playlist_id, language
        );
        let data: Vec<Value> = track_ids.iter().map(|id| playlist_track_ref(id)).collect();
        let body = serde_json::json!({ "data": data });

        let response = self
            .http
            .post(&url)
            .headers(self.build_headers())
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let json: Value = response.json().await.unwrap_or(Value::Null);
            anyhow::bail!(
                "Add to playlist failed: HTTP {} ({})",
                status,
                apple_error(&json, "no detail")
            );
        }
        Ok(())
    }
}

/// The `type` a track id takes inside a library playlist's `data` array.
///
/// Library songs carry an `i.` prefix and use `library-songs`; catalog songs use
/// numeric ids and `songs`. Sending the wrong type is rejected by Apple.
pub fn library_track_type(id: &str) -> &'static str {
    if id.starts_with("i.") {
        "library-songs"
    } else {
        "songs"
    }
}

/// One entry of a library playlist's `{ "data": [...] }` track array.
pub fn playlist_track_ref(id: &str) -> Value {
    serde_json::json!({ "id": id, "type": library_track_type(id) })
}

/// Pull Apple's error `detail` (or `title`) out of a failed response body.
fn apple_error(body: &Value, fallback: &str) -> String {
    body["errors"]
        .as_array()
        .and_then(|errors| errors.first())
        .and_then(|e| e["detail"].as_str().or_else(|| e["title"].as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| fallback.to_string())
}

pub fn parse_single_playlist_json(p: &Value) -> CloudPlaylist {
    let attrs = &p["attributes"];
    let mut tracks = Vec::new();

    if let Some(track_data) = p["relationships"]["tracks"]["data"].as_array() {
        for t in track_data {
            tracks.push(parse_single_song_json(t));
        }
    }

    let description = attrs["description"]["standard"]
        .as_str()
        .or_else(|| attrs["description"].as_str())
        .map(|s| s.to_string());

    CloudPlaylist {
        id: p["id"].as_str().unwrap_or("").to_string(),
        name: attrs["name"].as_str().unwrap_or("Untitled Playlist").to_string(),
        description,
        track_count: attrs["trackCount"].as_u64().unwrap_or(tracks.len() as u64) as u32,
        artwork_url: attrs["artwork"]["url"].as_str().map(|s| s.to_string()),
        can_edit: attrs["canEdit"].as_bool().unwrap_or(false),
        tracks,
    }
}

pub fn parse_catalog_search_json(json: &Value) -> CatalogSearchResult {
    let mut result = CatalogSearchResult::default();
    let res = &json["results"];

    if let Some(songs) = res["songs"]["data"].as_array() {
        for s in songs {
            result.songs.push(parse_single_song_json(s));
        }
    }

    if let Some(albums) = res["albums"]["data"].as_array() {
        for a in albums {
            result.albums.push(parse_single_album_json(a));
        }
    }

    if let Some(artists) = res["artists"]["data"].as_array() {
        for art in artists {
            result.artists.push(StoreArtist {
                id: art["id"].as_str().unwrap_or("").to_string(),
                name: art["attributes"]["name"].as_str().unwrap_or("").to_string(),
                url: art["attributes"]["url"].as_str().map(|s| s.to_string()),
            });
        }
    }

    result
}

pub fn parse_single_song_json(s: &Value) -> StoreSong {
    let attrs = &s["attributes"];
    let audio_traits = attrs["audioTraits"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();

    let has_lossless = audio_traits.contains(&"lossless") || audio_traits.contains(&"hi-res-lossless");
    let has_atmos = audio_traits.contains(&"spatial") || audio_traits.contains(&"atmos");

    let preview_url = attrs["previews"]
        .as_array()
        .and_then(|p| p.first())
        .and_then(|item| item["url"].as_str())
        .map(|url| url.to_string());

    StoreSong {
        id: s["id"].as_str().unwrap_or("").to_string(),
        title: attrs["name"].as_str().unwrap_or("Unknown Title").to_string(),
        artist: attrs["artistName"].as_str().unwrap_or("Unknown Artist").to_string(),
        album: attrs["albumName"].as_str().unwrap_or("").to_string(),
        duration_ms: attrs["durationInMillis"].as_u64().unwrap_or(0),
        track_number: attrs["trackNumber"].as_u64().unwrap_or(0) as u32,
        artwork_url: attrs["artwork"]["url"].as_str().map(|s| s.to_string()),
        preview_url,
        has_lossless,
        has_atmos,
        release_date: attrs["releaseDate"].as_str().map(|s| s.to_string()),
        genre: attrs["genreNames"].as_array().and_then(|a| a.first()).and_then(|g| g.as_str()).map(|s| s.to_string()),
    }
}

pub fn parse_single_album_json(a: &Value) -> StoreAlbum {
    let attrs = &a["attributes"];
    let mut tracks = Vec::new();

    if let Some(track_data) = a["relationships"]["tracks"]["data"].as_array() {
        for t in track_data {
            tracks.push(parse_single_song_json(t));
        }
    }

    StoreAlbum {
        id: a["id"].as_str().unwrap_or("").to_string(),
        title: attrs["name"].as_str().unwrap_or("Unknown Album").to_string(),
        artist: attrs["artistName"].as_str().unwrap_or("Unknown Artist").to_string(),
        track_count: attrs["trackCount"].as_u64().unwrap_or(tracks.len() as u64) as u32,
        release_date: attrs["releaseDate"].as_str().map(|s| s.to_string()),
        artwork_url: attrs["artwork"]["url"].as_str().map(|s| s.to_string()),
        tracks,
    }
}

#[cfg(test)]
mod playlist_write_tests {
    use super::{library_track_type, playlist_track_ref};

    #[test]
    fn library_ids_use_library_songs_type() {
        assert_eq!(library_track_type("i.mmpeDgktg0KWq42"), "library-songs");
    }

    #[test]
    fn catalog_ids_use_songs_type() {
        assert_eq!(library_track_type("1696166346"), "songs");
    }

    #[test]
    fn track_ref_carries_id_and_type() {
        let r = playlist_track_ref("1696166346");
        assert_eq!(r["id"], "1696166346");
        assert_eq!(r["type"], "songs");
    }
}
