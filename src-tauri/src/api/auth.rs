use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// Official Apple Music Web Client MusicKit anonymous bearer token
pub const DEFAULT_APPLE_MUSIC_DEV_TOKEN: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiIsImtpZCI6IldlYlBsYXlLaWQifQ.eyJpc3MiOiJBTVBXZWJQbGF5IiwiaWF0IjoxNzkyNzgwNTM0LCJleHAiOjE3OTg4Mjg1MzQsInJvb3RfaHR0cHNfb3JpZ2luIjpbImFwcGxlLmNvbSJdfQ.wcf_7U-zcgUjY2L8d4HWFkdxP1YSJCkkqEpEtVPKpRARrlaag0DHyIpRcoUEWZU8kJlvx5Q7ZNp04OabfuiuAw";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub music_user_token: Option<String>,
    pub storefront: String,
    pub language: String,
    pub dsid: Option<String>,
}

impl Default for UserSession {
    fn default() -> Self {
        Self {
            music_user_token: None,
            storefront: "id".to_string(), // Default storefront: Indonesia
            language: "en-GB".to_string(),
            dsid: None,
        }
    }
}

pub struct AuthManager {
    session_file: PathBuf,
    session: Mutex<UserSession>,
    developer_token: Mutex<String>,
}

impl AuthManager {
    pub fn new<P: AsRef<Path>>(session_file: P) -> Self {
        let path = session_file.as_ref().to_path_buf();
        let session = Self::load_from_disk(&path).unwrap_or_default();

        Self {
            session_file: path,
            session: Mutex::new(session),
            developer_token: Mutex::new(DEFAULT_APPLE_MUSIC_DEV_TOKEN.to_string()),
        }
    }

    pub fn default_path() -> PathBuf {
        let base_dir = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("AppleMusicRS");
        base_dir.join("session.json")
    }

    pub fn in_memory() -> Self {
        let temp_file = std::env::temp_dir().join(format!("test_session_{}.json", uuid::Uuid::new_v4()));
        Self {
            session_file: temp_file,
            session: Mutex::new(UserSession::default()),
            developer_token: Mutex::new(DEFAULT_APPLE_MUSIC_DEV_TOKEN.to_string()),
        }
    }

    fn load_from_disk(path: &Path) -> Option<UserSession> {
        let mut file = File::open(path).ok()?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).ok()?;
        serde_json::from_str(&contents).ok()
    }

    pub fn save_to_disk(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.session_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let session = self.session.lock().unwrap();
        let data = serde_json::to_string_pretty(&*session)?;
        let mut file = File::create(&self.session_file)?;
        file.write_all(data.as_bytes())?;
        file.flush()?;
        Ok(())
    }

    pub fn set_user_token(&self, token: Option<String>) -> anyhow::Result<()> {
        {
            let mut session = self.session.lock().unwrap();
            session.music_user_token = token.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        }
        self.save_to_disk()
    }

    pub fn set_storefront(&self, storefront: String, language: String) -> anyhow::Result<()> {
        {
            let mut session = self.session.lock().unwrap();
            session.storefront = storefront;
            session.language = language;
        }
        self.save_to_disk()
    }

    pub fn set_developer_token(&self, token: String) {
        let mut dev = self.developer_token.lock().unwrap();
        *dev = token;
    }

    pub fn developer_token(&self) -> String {
        self.developer_token.lock().unwrap().clone()
    }

    pub fn user_token(&self) -> Option<String> {
        self.session.lock().unwrap().music_user_token.clone()
    }

    pub fn storefront(&self) -> String {
        self.session.lock().unwrap().storefront.clone()
    }

    pub fn language(&self) -> String {
        self.session.lock().unwrap().language.clone()
    }

    pub fn is_authenticated(&self) -> bool {
        self.session.lock().unwrap().music_user_token.is_some()
    }

    pub fn sign_out(&self) -> anyhow::Result<()> {
        {
            let mut session = self.session.lock().unwrap();
            session.music_user_token = None;
            session.dsid = None;
        }
        self.save_to_disk()
    }

    pub fn masked_user_token(&self) -> Option<String> {
        let token = self.user_token()?;
        if token.len() > 10 {
            Some(format!("{}...{}", &token[..4], &token[token.len() - 4..]))
        } else {
            Some("****".to_string())
        }
    }

    /// Automatically extracts and refreshes the official public MusicKit token from music.apple.com
    pub async fn refresh_developer_token_from_web(&self) -> Option<String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(6))
            .build()
            .ok()?;
        let html = client.get("https://music.apple.com/us/browse").send().await.ok()?.text().await.ok()?;
        let idx = html.find("/assets/index~")?;
        let end_idx = html[idx..].find(".js")?;
        let js_path = &html[idx..idx + end_idx + 3];
        let js_url = format!("https://music.apple.com{}", js_path);
        let js_code = client.get(&js_url).send().await.ok()?.text().await.ok()?;
        let jwt_prefix = "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiIsImtpZCI6IldlYlBsYXlLaWQifQ.";
        let start = js_code.find(jwt_prefix)?;
        let slice = &js_code[start..];
        let end = slice.find('"').or_else(|| slice.find('\''))?;
        let token = &slice[..end];
        if token.len() > 50 {
            *self.developer_token.lock().unwrap() = token.to_string();
            return Some(token.to_string());
        }
        None
    }
}
