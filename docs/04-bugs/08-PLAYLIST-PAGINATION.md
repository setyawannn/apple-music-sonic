# Bug 08 — Long playlists lose their tail: one page was treated as the whole list

> Status: **fixed** (2026-11-15)
> Symptom reported as "my playlist should have so many songs but a few of them don't appear".

## Symptom

A library playlist with more than 100 songs opened with only the first 100 rows. No error, no
toast, no log line — the list simply ended early, so it looked like a handful of songs had gone
missing. The same truncation hit **Your Library**, which quietly showed only the first 100
iCloud songs.

## Root cause

`ApiClient::get_playlist_tracks` built one URL and read one page:

```rust
// before
let url = format!(
    "{}/v1/me/library/playlists/{}/tracks?limit=100&l={}",
    self.base_url, playlist_id, language
);
let json: Value = response.json().await?;
if let Some(data) = json["data"].as_array() {
    for item in data { songs.push(parse_single_song_json(item)); }
}
```

Apple caps list endpoints at **100 items per request** and returns the rest through a `next`
cursor:

```json
{
  "data": [ ... 100 items ... ],
  "next": "/v1/me/library/playlists/p.abc123/tracks?offset=100&limit=100"
}
```

Nothing followed `next`. Item 101 onwards existed only on Apple's side. `get_personal_library_songs`
(`/v1/me/library/songs?limit=100`) and `get_user_playlists` had the identical shape and the
identical bug.

This is the same class of mistake as bug 05: a query parameter that Apple accepts but answers
narrower than the code assumes.

## Fix

One paginator in `api/client.rs`, used by every list read:

```rust
const PAGE_LIMIT: usize = 100;
const MAX_PAGES: usize = 200;

async fn fetch_all_pages(&self, path: &str) -> anyhow::Result<Vec<Value>> {
    let mut items = Vec::new();
    let mut offset = 0usize;
    let mut cursor: Option<String> = None;
    let mut previous_first_id: Option<String> = None;

    for _ in 0..MAX_PAGES {
        let url = match cursor.take() {
            Some(next) => next,                       // Apple's cursor wins
            None => self.list_url(path, offset),      // else page by offset
        };
        let page = self.get_json(&url).await?;
        let Some(data) = page["data"].as_array() else { break };

        // Same first id twice = the server ignored `offset`; stop instead of looping.
        let first_id = data.first().and_then(|i| i["id"].as_str()).map(str::to_string);
        if first_id.is_some() && first_id == previous_first_id { break; }
        previous_first_id = first_id;

        items.extend(data.iter().cloned());
        if data.len() < PAGE_LIMIT { break; }
        offset += data.len();
        cursor = self.next_page_url(&page);
    }
    Ok(items)
}
```

Details that matter:

- `next` is a **path**, not an absolute URL, so it is joined onto `base_url`.
- A short page ends the loop; a full page means "there may be more".
- Two guards against an infinite UI spinner: `MAX_PAGES` (200) and a repeated-first-id check
  for servers that ignore `offset`.
- Duplicates inside a playlist are legitimate (the same song twice), so nothing is deduplicated.

`get_cloud_playlist_tracks` now also logs the count, so the next report of this kind is
verifiable from the console:

```
[PLAYLIST] 137 tracks in p.abc123
```

## Tests

Two acceptance tests cover both paging routes with the existing mock HTTP server:

| Test | Covers |
|------|--------|
| `test_api_pos_09_playlist_tracks_follow_next_cursor` | 100 items + `next` → 3 more = 103, and item 100 is page two |
| `test_api_pos_10_pagination_falls_back_to_offset` | no `next` field → offset advanced by page size = 105 |

## Rule

**Any Apple list endpoint is paged.** Read through `fetch_all_pages`; never assume one response
is the whole collection. `limit` is a page size, not a result size.
