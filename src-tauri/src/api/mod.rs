pub mod auth;
pub mod client;
pub mod local_session;
pub mod lyrics;
pub mod models;

pub use auth::{AuthManager, UserSession};
pub use client::ApiClient;
pub use local_session::{LocalAppleSession, LocalSessionReader, SessionError};
pub use lyrics::{LyricLine, LyricSegment, Lyrics, LyricsTiming};
pub use models::*;
