use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoreSong {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub track_number: u32,
    pub artwork_url: Option<String>,
    pub preview_url: Option<String>,
    pub has_lossless: bool,
    pub has_atmos: bool,
    pub release_date: Option<String>,
    pub genre: Option<String>,
}

impl StoreSong {
    pub fn formatted_artwork_url(&self, width: u32, height: u32, format: &str) -> Option<String> {
        self.artwork_url.as_ref().map(|template| {
            template
                .replace("{w}", &width.to_string())
                .replace("{h}", &height.to_string())
                .replace("{f}", format)
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoreAlbum {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub track_count: u32,
    pub release_date: Option<String>,
    pub artwork_url: Option<String>,
    pub tracks: Vec<StoreSong>,
}

impl StoreAlbum {
    pub fn formatted_artwork_url(&self, width: u32, height: u32, format: &str) -> Option<String> {
        self.artwork_url.as_ref().map(|template| {
            template
                .replace("{w}", &width.to_string())
                .replace("{h}", &height.to_string())
                .replace("{f}", format)
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreArtist {
    pub id: String,
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CatalogSearchResult {
    pub songs: Vec<StoreSong>,
    pub albums: Vec<StoreAlbum>,
    pub artists: Vec<StoreArtist>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SearchSuggestions {
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CatalogCharts {
    pub songs: Vec<StoreSong>,
    pub albums: Vec<StoreAlbum>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthState {
    pub is_authenticated: bool,
    pub storefront: String,
    pub language: String,
    pub user_token_masked: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudPlaylist {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub track_count: u32,
    pub artwork_url: Option<String>,
    pub can_edit: bool,
    pub tracks: Vec<StoreSong>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncStats {
    pub playlists_synced: usize,
    pub tracks_synced: usize,
    pub error: Option<String>,
}
