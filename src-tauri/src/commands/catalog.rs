//! Catalog browse + search + the user's iCloud library.

use crate::api::{CatalogCharts, CatalogSearchResult, SearchSuggestions, StoreAlbum, StoreSong};
use crate::{AppResult, AppState};
use tauri::State;

#[tauri::command]
pub async fn search_catalog(
    state: State<'_, AppState>,
    query: String,
    types: Option<String>,
    limit: Option<u32>,
) -> AppResult<CatalogSearchResult> {
    Ok(state
        .api_client
        .search_catalog(&query, types.as_deref(), limit.unwrap_or(25))
        .await?)
}

#[tauri::command]
pub async fn get_search_suggestions(
    state: State<'_, AppState>,
    query: String,
) -> AppResult<SearchSuggestions> {
    Ok(state.api_client.get_search_suggestions(&query).await?)
}

#[tauri::command]
pub async fn get_catalog_album(state: State<'_, AppState>, id: String) -> AppResult<StoreAlbum> {
    Ok(state.api_client.get_catalog_album(&id).await?)
}

#[tauri::command]
pub async fn get_charts(
    state: State<'_, AppState>,
    types: Option<String>,
    limit: Option<u32>,
) -> AppResult<CatalogCharts> {
    Ok(state
        .api_client
        .get_charts(types.as_deref(), limit.unwrap_or(20))
        .await?)
}

#[tauri::command]
pub async fn get_personal_library(state: State<'_, AppState>) -> AppResult<Vec<StoreSong>> {
    Ok(state.api_client.get_personal_library_songs().await?)
}

#[tauri::command]
pub async fn get_recommendations(state: State<'_, AppState>) -> AppResult<Vec<StoreAlbum>> {
    Ok(state.api_client.get_personal_recommendations().await?)
}
