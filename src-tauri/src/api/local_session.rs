use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("Cookies database not found at: {0}")]
    NotFound(PathBuf),
    #[error("SQLite query error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("No valid Apple Music session cookies found in database")]
    NoSessionFound,
    #[error("Decryption error: {0}")]
    DecryptionFailed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalAppleSession {
    pub media_user_token: Option<String>,
    pub storefront: Option<String>,
    pub itctx: Option<String>,
    pub dsid: Option<String>,
    pub all_cookies: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct LocalSessionReader;

impl LocalSessionReader {
    pub fn new() -> Self {
        Self
    }

    /// Extract session data from specified Cookies SQLite database
    pub fn extract_from_cookies_db(&self, db_path: &Path) -> Result<LocalAppleSession, SessionError> {
        if !db_path.exists() {
            return Err(SessionError::NotFound(db_path.to_path_buf()));
        }

        // Open database using Immutable URI mode (read-only, no lock contention with official app)
        let normalized = db_path.to_string_lossy().replace('\\', "/");
        let uri = format!("file:///{}?immutable=1&mode=ro", normalized);

        let conn = Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .or_else(|_| Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY))?;

        let mut stmt = conn.prepare(
            "SELECT host_key, name, path, value, encrypted_value FROM cookies WHERE host_key LIKE '%apple.com%'",
        )?;

        let mut session = LocalAppleSession::default();
        let mut count = 0;

        let rows = stmt.query_map([], |row| {
            let _host: String = row.get(0)?;
            let name: String = row.get(1)?;
            let _path: String = row.get(2)?;
            let val_str: String = row.get(3)?;
            let enc_bytes: Vec<u8> = row.get(4)?;
            Ok((name, val_str, enc_bytes))
        })?;

        for item in rows {
            let (name, val_str, enc_bytes) = item?;

            let final_val = if !val_str.is_empty() {
                val_str
            } else if !enc_bytes.is_empty() {
                Self::decrypt_cookie_value(&enc_bytes).unwrap_or_default()
            } else {
                String::new()
            };

            if !final_val.is_empty() {
                count += 1;
                let lower_name = name.to_lowercase();
                if lower_name == "media-user-token" || lower_name == "mut" {
                    session.media_user_token = Some(final_val.clone());
                } else if lower_name == "itctx" {
                    session.itctx = Some(final_val.clone());
                } else if lower_name == "x-apple-store-front" || lower_name == "storefront" {
                    session.storefront = Some(final_val.clone());
                } else if lower_name == "dsid" || lower_name == "x-dsid" {
                    session.dsid = Some(final_val.clone());
                }
                session.all_cookies.push((name, final_val));
            }
        }

        if count == 0 {
            return Err(SessionError::NoSessionFound);
        }

        Ok(session)
    }

    /// Extract session by navigating from Apple Music LocalState folder
    pub fn extract_from_local_state(&self, local_state_dir: &Path) -> Result<LocalAppleSession, SessionError> {
        let cookie_path = local_state_dir
            .join("EBWebView")
            .join("Default")
            .join("Network")
            .join("Cookies");

        if cookie_path.exists() {
            return self.extract_from_cookies_db(&cookie_path);
        }

        // Alternative location fallback
        let alt_path = local_state_dir
            .join("EBWebView")
            .join("Default")
            .join("Cookies");

        if alt_path.exists() {
            return self.extract_from_cookies_db(&alt_path);
        }

        Err(SessionError::NotFound(cookie_path))
    }

    /// Decrypt DPAPI or raw cookie value on Windows
    fn decrypt_cookie_value(data: &[u8]) -> Option<String> {
        #[cfg(windows)]
        {
            #[repr(C)]
            struct DataBlob {
                cb_data: u32,
                pb_data: *mut u8,
            }

            #[link(name = "crypt32")]
            extern "system" {
                fn CryptUnprotectData(
                    pDataIn: *const DataBlob,
                    ppszDataDescr: *mut *mut u16,
                    pOptionalEntropy: *const DataBlob,
                    pvReserved: *mut std::ffi::c_void,
                    pPromptStruct: *mut std::ffi::c_void,
                    dwFlags: u32,
                    pDataOut: *mut DataBlob,
                ) -> i32;
            }

            #[link(name = "kernel32")]
            extern "system" {
                fn LocalFree(hMem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
            }

            let in_blob = DataBlob {
                cb_data: data.len() as u32,
                pb_data: data.as_ptr() as *mut u8,
            };
            let mut out_blob = DataBlob {
                cb_data: 0,
                pb_data: std::ptr::null_mut(),
            };

            let res = unsafe {
                CryptUnprotectData(
                    &in_blob,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                    &mut out_blob,
                )
            };

            if res != 0 && !out_blob.pb_data.is_null() && out_blob.cb_data > 0 {
                let decrypted_slice = unsafe {
                    std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize)
                };
                let result = String::from_utf8(decrypted_slice.to_vec()).ok();
                unsafe { LocalFree(out_blob.pb_data as *mut std::ffi::c_void) };
                if result.is_some() {
                    return result;
                }
            }
        }

        // Fallback: UTF-8 lossy conversion if stored unencrypted
        if let Ok(s) = std::str::from_utf8(data) {
            let trimmed = s.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_mock_cookies_db(entries: &[(&str, &str, &str)]) -> PathBuf {
        let db_path = std::env::temp_dir().join(format!("mock_cookies_{}.sqlite", uuid::Uuid::new_v4()));
        let conn = Connection::open(&db_path).expect("open mock cookies db");

        conn.execute(
            "CREATE TABLE cookies (
                creation_utc INTEGER NOT NULL,
                host_key TEXT NOT NULL,
                top_frame_site_key TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL,
                value TEXT NOT NULL,
                encrypted_value BLOB NOT NULL,
                path TEXT NOT NULL,
                expires_utc INTEGER NOT NULL DEFAULT 0,
                is_secure INTEGER NOT NULL DEFAULT 0,
                is_httponly INTEGER NOT NULL DEFAULT 0,
                last_access_utc INTEGER NOT NULL DEFAULT 0,
                has_expires INTEGER NOT NULL DEFAULT 0,
                is_persistent INTEGER NOT NULL DEFAULT 0,
                priority INTEGER NOT NULL DEFAULT 1,
                samesite INTEGER NOT NULL DEFAULT 0,
                source_scheme INTEGER NOT NULL DEFAULT 0,
                source_port INTEGER NOT NULL DEFAULT 443,
                last_update_utc INTEGER NOT NULL DEFAULT 0,
                source_type INTEGER NOT NULL DEFAULT 0,
                has_cross_site_ancestor INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .expect("create cookies table");

        for &(host, name, val) in entries {
            conn.execute(
                "INSERT INTO cookies (creation_utc, host_key, name, value, encrypted_value, path)
                 VALUES (1000, ?1, ?2, ?3, X'', '/')",
                [host, name, val],
            )
            .expect("insert cookie");
        }

        db_path
    }

    // ─── Positive Tests (POS-01 .. POS-04) ──────────────────────────────

    #[test]
    fn test_ses_pos_01_read_cookies_from_isolated_mock() {
        let db = create_mock_cookies_db(&[
            (".apple.com", "media-user-token", "test_mut_sample_token_12345"),
            (".apple.com", "itctx", "sample_itctx_value"),
            (".apple.com", "x-apple-store-front", "143476-2,42"),
        ]);

        let reader = LocalSessionReader::new();
        let session = reader.extract_from_cookies_db(&db).expect("extract cookies");

        assert_eq!(session.media_user_token.as_deref(), Some("test_mut_sample_token_12345"));
        assert_eq!(session.itctx.as_deref(), Some("sample_itctx_value"));
        assert_eq!(session.storefront.as_deref(), Some("143476-2,42"));
        assert_eq!(session.all_cookies.len(), 3);

        let _ = fs::remove_file(&db);
    }

    #[test]
    fn test_ses_pos_02_immutable_uri_mode_prevents_lock_failure() {
        let db = create_mock_cookies_db(&[(".apple.com", "itctx", "active_session")]);

        // Keep a connection open to simulate official app running
        let _open_conn = Connection::open(&db).expect("open concurrent conn");

        let reader = LocalSessionReader::new();
        let session = reader.extract_from_cookies_db(&db).expect("extract with open handle");

        assert_eq!(session.itctx.as_deref(), Some("active_session"));

        drop(_open_conn);
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn test_ses_pos_03_multiple_apple_domain_filtering() {
        let db = create_mock_cookies_db(&[
            (".apple.com", "apple_cookie", "val_apple"),
            ("music.apple.com", "media-user-token", "val_mut"),
            (".google.com", "google_cookie", "val_google"),
            (".spotify.com", "spotify_cookie", "val_spotify"),
        ]);

        let reader = LocalSessionReader::new();
        let session = reader.extract_from_cookies_db(&db).expect("extract filtered cookies");

        assert_eq!(session.media_user_token.as_deref(), Some("val_mut"));
        assert_eq!(session.all_cookies.len(), 2);
        assert!(session.all_cookies.iter().any(|(n, _)| n == "apple_cookie"));
        assert!(session.all_cookies.iter().any(|(n, _)| n == "media-user-token"));
        assert!(!session.all_cookies.iter().any(|(n, _)| n == "google_cookie"));

        let _ = fs::remove_file(&db);
    }

    #[test]
    fn test_ses_pos_04_auto_detection_from_local_state_dir() {
        let unique = uuid::Uuid::new_v4();
        let mock_local_state = std::env::temp_dir().join(format!("MockLocalState_{}", unique));
        let network_dir = mock_local_state.join("EBWebView").join("Default").join("Network");
        fs::create_dir_all(&network_dir).expect("create network dir");

        let db = network_dir.join("Cookies");
        let conn = Connection::open(&db).expect("open cookies db in hierarchy");
        conn.execute(
            "CREATE TABLE cookies (host_key TEXT, name TEXT, path TEXT, value TEXT, encrypted_value BLOB)",
            [],
        )
        .expect("create table");
        conn.execute(
            "INSERT INTO cookies VALUES ('.apple.com', 'itctx', '/', 'auto_detected_itctx', X'')",
            [],
        )
        .expect("insert row");
        drop(conn);

        let reader = LocalSessionReader::new();
        let session = reader.extract_from_local_state(&mock_local_state).expect("extract from hierarchy");

        assert_eq!(session.itctx.as_deref(), Some("auto_detected_itctx"));

        let _ = fs::remove_dir_all(&mock_local_state);
    }

    // ─── Negative Tests (NEG-01 .. NEG-04) ──────────────────────────────

    #[test]
    fn test_ses_neg_01_non_existent_sqlite_database_file() {
        let fake = PathBuf::from("non_existent_cookies_file_12345.sqlite");
        let reader = LocalSessionReader::new();
        let err = reader.extract_from_cookies_db(&fake).unwrap_err();
        match err {
            SessionError::NotFound(p) => assert_eq!(p, fake),
            other => panic!("Expected NotFound, got: {:?}", other),
        }
    }

    #[test]
    fn test_ses_neg_02_corrupted_sqlite_database_file() {
        let bad_file = std::env::temp_dir().join(format!("corrupt_{}.sqlite", uuid::Uuid::new_v4()));
        fs::write(&bad_file, b"THIS_IS_NOT_A_VALID_SQLITE_HEADER").expect("write garbage");

        let reader = LocalSessionReader::new();
        let err = reader.extract_from_cookies_db(&bad_file).unwrap_err();
        match err {
            SessionError::Sqlite(_) => {}
            other => panic!("Expected Sqlite error, got: {:?}", other),
        }

        let _ = fs::remove_file(&bad_file);
    }

    #[test]
    fn test_ses_neg_03_empty_table_or_no_apple_cookies_found() {
        let db = create_mock_cookies_db(&[
            (".example.com", "other_cookie", "some_value"),
            (".github.com", "user", "octocat"),
        ]);

        let reader = LocalSessionReader::new();
        let err = reader.extract_from_cookies_db(&db).unwrap_err();
        match err {
            SessionError::NoSessionFound => {}
            other => panic!("Expected NoSessionFound, got: {:?}", other),
        }

        let _ = fs::remove_file(&db);
    }

    #[test]
    fn test_ses_neg_04_missing_cookies_table_schema() {
        let db = std::env::temp_dir().join(format!("wrong_schema_{}.sqlite", uuid::Uuid::new_v4()));
        let conn = Connection::open(&db).expect("create wrong schema db");
        conn.execute("CREATE TABLE wrong_table (id INTEGER)", []).expect("create table");
        drop(conn);

        let reader = LocalSessionReader::new();
        let err = reader.extract_from_cookies_db(&db).unwrap_err();
        match err {
            SessionError::Sqlite(e) => {
                let msg = e.to_string();
                assert!(msg.contains("no such table") || msg.contains("wrong_table"));
            }
            other => panic!("Expected Sqlite error for missing table, got: {:?}", other),
        }

        let _ = fs::remove_file(&db);
    }
}
