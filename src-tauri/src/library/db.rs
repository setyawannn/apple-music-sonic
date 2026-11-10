use super::{Album, Artist, Playlist, Track};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    pub fn in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();

        // High performance SQLite pragmas
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA cache_size = -64000;

             CREATE TABLE IF NOT EXISTS tracks (
                 id TEXT PRIMARY KEY,
                 title TEXT NOT NULL,
                 artist TEXT NOT NULL,
                 album TEXT NOT NULL,
                 album_artist TEXT NOT NULL DEFAULT '',
                 track_number INTEGER NOT NULL DEFAULT 0,
                 disc_number INTEGER NOT NULL DEFAULT 1,
                 duration_secs REAL NOT NULL DEFAULT 0.0,
                 file_path TEXT UNIQUE NOT NULL,
                 file_size INTEGER NOT NULL,
                 genre TEXT NOT NULL DEFAULT '',
                 year INTEGER NOT NULL DEFAULT 0,
                 art_path TEXT,
                 file_modified INTEGER NOT NULL DEFAULT 0,
                 date_added INTEGER NOT NULL DEFAULT 0
             );

             CREATE INDEX IF NOT EXISTS idx_tracks_file_path ON tracks(file_path);
             CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);
             CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
             CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);

             CREATE TABLE IF NOT EXISTS playlists (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 description TEXT NOT NULL DEFAULT '',
                 is_folder INTEGER NOT NULL DEFAULT 0,
                 parent_id TEXT,
                 date_created INTEGER NOT NULL DEFAULT 0
             );

             CREATE TABLE IF NOT EXISTS playlist_tracks (
                 playlist_id TEXT NOT NULL,
                 track_id TEXT NOT NULL,
                 position INTEGER NOT NULL,
                 PRIMARY KEY (playlist_id, position),
                 FOREIGN KEY(playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
                 FOREIGN KEY(track_id) REFERENCES tracks(id) ON DELETE CASCADE
             );",
        )?;

        Ok(())
    }

    pub fn insert_or_replace_track(&self, track: &Track, file_modified: u64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT INTO tracks (
                id, title, artist, album, album_artist, track_number, disc_number,
                duration_secs, file_path, file_size, genre, year, art_path, file_modified, date_added
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(file_path) DO UPDATE SET
                title = excluded.title,
                artist = excluded.artist,
                album = excluded.album,
                album_artist = excluded.album_artist,
                track_number = excluded.track_number,
                disc_number = excluded.disc_number,
                duration_secs = excluded.duration_secs,
                file_size = excluded.file_size,
                genre = excluded.genre,
                year = excluded.year,
                art_path = excluded.art_path,
                file_modified = excluded.file_modified;",
            params![
                track.id,
                track.title,
                track.artist,
                track.album,
                track.album_artist,
                track.track_number,
                track.disc_number,
                track.duration_secs,
                track.file_path,
                track.file_size,
                track.genre,
                track.year,
                track.art_path,
                file_modified as i64,
                now,
            ],
        )?;

        Ok(())
    }

    pub fn get_all_tracks(&self) -> anyhow::Result<Vec<Track>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, artist, album, album_artist, track_number, disc_number,
                    duration_secs, file_path, file_size, genre, year, art_path
             FROM tracks ORDER BY artist COLLATE NOCASE, album COLLATE NOCASE, track_number ASC",
        )?;

        let track_iter = stmt.query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                album: row.get(3)?,
                album_artist: row.get(4)?,
                track_number: row.get(5)?,
                disc_number: row.get(6)?,
                duration_secs: row.get(7)?,
                file_path: row.get(8)?,
                file_size: row.get(9)?,
                genre: row.get(10)?,
                year: row.get(11)?,
                art_path: row.get(12)?,
            })
        })?;

        let mut tracks = Vec::new();
        for t in track_iter {
            tracks.push(t?);
        }
        Ok(tracks)
    }

    pub fn get_track_by_id(&self, id: &str) -> anyhow::Result<Option<Track>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, artist, album, album_artist, track_number, disc_number,
                    duration_secs, file_path, file_size, genre, year, art_path
             FROM tracks WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Track {
                id: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                album: row.get(3)?,
                album_artist: row.get(4)?,
                track_number: row.get(5)?,
                disc_number: row.get(6)?,
                duration_secs: row.get(7)?,
                file_path: row.get(8)?,
                file_size: row.get(9)?,
                genre: row.get(10)?,
                year: row.get(11)?,
                art_path: row.get(12)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn delete_track_by_path(&self, file_path: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM tracks WHERE file_path = ?1", params![file_path])?;
        Ok(())
    }

    pub fn get_cached_file_stats(&self) -> anyhow::Result<HashMap<String, (u64, u64)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT file_path, file_size, file_modified FROM tracks")?;
        let rows = stmt.query_map([], |row| {
            let path: String = row.get(0)?;
            let size: i64 = row.get(1)?;
            let mtime: i64 = row.get(2)?;
            Ok((path, (size as u64, mtime as u64)))
        })?;

        let mut stats = HashMap::new();
        for r in rows {
            let (path, stat) = r?;
            stats.insert(path, stat);
        }
        Ok(stats)
    }

    pub fn prune_missing_tracks(&self, active_paths: &[String]) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut count = 0;

        let mut stmt = conn.prepare("SELECT file_path FROM tracks")?;
        let paths: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let active_set: std::collections::HashSet<&str> =
            active_paths.iter().map(|s| s.as_str()).collect();

        let mut del_stmt = conn.prepare("DELETE FROM tracks WHERE file_path = ?1")?;
        for p in paths {
            if !active_set.contains(p.as_str()) {
                del_stmt.execute(params![p])?;
                count += 1;
            }
        }

        Ok(count)
    }

    pub fn get_albums(&self) -> anyhow::Result<Vec<Album>> {
        let tracks = self.get_all_tracks()?;
        let mut album_map: HashMap<String, Vec<Track>> = HashMap::new();

        for track in tracks {
            let artist_name = if track.album_artist.is_empty() {
                &track.artist
            } else {
                &track.album_artist
            };
            let key = format!("{} --- {}", track.album, artist_name);
            album_map.entry(key).or_default().push(track);
        }

        let mut albums = Vec::new();
        for (_, mut group) in album_map {
            group.sort_by_key(|t| (t.disc_number, t.track_number));
            let first = &group[0];
            let artist = if first.album_artist.is_empty() {
                first.artist.clone()
            } else {
                first.album_artist.clone()
            };

            albums.push(Album {
                id: Uuid::new_v4().to_string(),
                title: first.album.clone(),
                artist,
                year: first.year,
                track_count: group.len() as u32,
                art_path: first.art_path.clone(),
                tracks: group,
            });
        }

        albums.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(albums)
    }

    pub fn get_artists(&self) -> anyhow::Result<Vec<Artist>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT artist, COUNT(id) as track_count, COUNT(DISTINCT album) as album_count
             FROM tracks
             GROUP BY artist
             ORDER BY artist COLLATE NOCASE ASC",
        )?;

        let artist_iter = stmt.query_map([], |row| {
            Ok(Artist {
                name: row.get(0)?,
                track_count: row.get(1)?,
                album_count: row.get(2)?,
            })
        })?;

        let mut artists = Vec::new();
        for a in artist_iter {
            artists.push(a?);
        }
        Ok(artists)
    }

    pub fn create_playlist(&self, id: &str, name: &str, description: &str) -> anyhow::Result<Playlist> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO playlists (id, name, description, is_folder, parent_id, date_created)
             VALUES (?1, ?2, ?3, 0, NULL, ?4)",
            params![id, name, description, now],
        )?;

        Ok(Playlist {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            tracks: Vec::new(),
            is_folder: false,
            parent_id: None,
        })
    }

    pub fn get_playlists(&self) -> anyhow::Result<Vec<Playlist>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, p.description, p.is_folder, p.parent_id
             FROM playlists p
             ORDER BY p.name COLLATE NOCASE ASC",
        )?;

        let playlist_iter = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let description: String = row.get(2)?;
            let is_folder: bool = row.get::<_, i32>(3)? != 0;
            let parent_id: Option<String> = row.get(4)?;

            Ok((id, name, description, is_folder, parent_id))
        })?;

        let mut playlists = Vec::new();
        for p in playlist_iter {
            let (id, name, description, is_folder, parent_id) = p?;
            let mut track_stmt = conn.prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position ASC",
            )?;
            let track_ids: Vec<String> = track_stmt
                .query_map(params![id], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();

            playlists.push(Playlist {
                id,
                name,
                description,
                tracks: track_ids,
                is_folder,
                parent_id,
            });
        }

        Ok(playlists)
    }

    pub fn add_track_to_playlist(&self, playlist_id: &str, track_id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let mut pos_stmt = conn.prepare(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?1",
        )?;
        let next_pos: i32 = pos_stmt.query_row(params![playlist_id], |row| row.get(0))?;

        conn.execute(
            "INSERT OR REPLACE INTO playlist_tracks (playlist_id, track_id, position)
             VALUES (?1, ?2, ?3)",
            params![playlist_id, track_id, next_pos],
        )?;
        Ok(())
    }

    pub fn remove_track_from_playlist(&self, playlist_id: &str, track_id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2",
            params![playlist_id, track_id],
        )?;
        Ok(())
    }

    pub fn delete_playlist(&self, playlist_id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM playlists WHERE id = ?1", params![playlist_id])?;
        Ok(())
    }
}
