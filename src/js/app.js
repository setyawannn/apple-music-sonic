import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// ─── State ─────────────────────────────────────────────────────
const state = {
  currentView: "listen-now",
  tracks: [],
  albums: [],
  artists: [],
  playlists: [],
  cloudPlaylists: [],
  cloudTracks: [],
  isSyncing: false,
  auth: {
    isAuthenticated: false,
    storefront: "id",
    language: "en-GB",
    userTokenMasked: null,
  },
  playback: {
    isPlaying: false,
    currentTrackId: null,
    currentTrackName: "No Track Selected",
    currentArtist: "Open your library to start playing",
    position: 0,
    duration: 0,
    volume: 0.8,
    repeatMode: "Off",
    isShuffle: false,
  },
  // Hidden MusicKit player's own state, mirrored from `player-state`.
  streamingActive: false,
  streamIsPlaying: false,
  streamPosition: 0,
  streamDuration: 0,
  streamAnchor: 0,
  streamShuffle: false,
  streamRepeat: 0, // 0 off, 1 one, 2 all
  streamVolume: null,
  streamQueueLength: 0,
};

// ─── DOM References ────────────────────────────────────────────
const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => document.querySelectorAll(sel);

const mainView = $("#main-view");
const searchInput = $("#search-input");
const searchSuggestions = $("#search-suggestions");
const authBtn = $("#auth-btn");
const authLabel = $("#auth-label");
const authModal = $("#auth-modal");
const modalCloseBtn = $("#modal-close-btn");
const modalAuthBody = $("#modal-auth-body");
const playBtn = $("#play-btn");
const playIcon = $("#play-icon");
const prevBtn = $("#prev-btn");
const nextBtn = $("#next-btn");
const shuffleBtn = $("#shuffle-btn");
const repeatBtn = $("#repeat-btn");
const nowPlayingTitle = $("#now-playing-title");
const nowPlayingArtist = $("#now-playing-artist");
const currentTime = $("#current-time");
const totalTime = $("#total-time");
const progressBar = $("#progress-bar");
const progressFill = $("#progress-fill");
const volumeBar = $("#volume-bar");
const volumeFill = $("#volume-fill");
const trackCount = $("#track-count");
const playlistList = $("#playlist-list");
const navItems = $$(".nav-item");

// ─── Navigation ────────────────────────────────────────────────
document.querySelector(".sidebar-nav.library-nav")?.addEventListener("click", (e) => {
  const item = e.target.closest(".nav-item");
  if (!item) return;
  const view = item.dataset.view;
  if (!view) return;

  state.currentView = view;
  navItems.forEach((n) => n.classList.remove("active"));
  item.classList.add("active");

  switch (view) {
    case "library":
      renderLibrary();
      break;
    case "albums":
      renderAlbums();
      break;
    case "playlists":
      renderPlaylists();
      break;
    case "artists":
      renderArtists();
      break;
    default:
      renderWelcome();
  }
});

document.querySelectorAll(".sidebar-nav:not(.library-nav) .nav-item").forEach((item) => {
  item.addEventListener("click", () => {
    const view = item.dataset.view;
    state.currentView = view;
    navItems.forEach((n) => n.classList.remove("active"));
    item.classList.add("active");

    if (view === "browse") {
      renderBrowse();
    } else if (view === "radio") {
      renderRadio();
    } else {
      renderListenNow();
    }
  });
});

// ─── Authentication Management ─────────────────────────────────
async function checkAuthState() {
  try {
    let auth = await invoke("get_auth_state");
    
    // Auto-detect and import existing Apple Music session from local PC cookies if unauthenticated
    if (!auth.is_authenticated) {
      try {
        const bridge = await invoke("get_native_bridge_status");
        if (bridge && bridge.session_detected) {
          const importRes = await invoke("import_local_apple_session");
          if (importRes && importRes.success && importRes.has_user_token) {
            console.log("Successfully imported Apple Music session from local PC cookies");
            auth = await invoke("get_auth_state");
          }
        }
      } catch (err) {
        console.debug("Local Apple Music session auto-import skipped:", err);
      }
    }

    state.auth.isAuthenticated = auth.is_authenticated;
    state.auth.storefront = auth.storefront;
    state.auth.language = auth.language;
    state.auth.userTokenMasked = auth.user_token_masked;

    if (auth.is_authenticated) {
      authBtn.classList.add("authenticated");
      authLabel.textContent = "Apple ID";
      syncCloudPlaylists();
      // Preload the iCloud library so the Library badge and the library views
      // are instant the first time they are opened.
      ensureCloudLibrary().catch(() => {});
    } else {
      authBtn.classList.remove("authenticated");
      authLabel.textContent = "Sign In";
    }
  } catch (err) {
    console.error("Failed to get auth state:", err);
  }
}

async function syncCloudPlaylists() {
  if (!state.auth.isAuthenticated || state.isSyncing) return;
  state.isSyncing = true;
  try {
    const playlists = await invoke("get_cloud_playlists");
    if (Array.isArray(playlists)) {
      state.cloudPlaylists = playlists;
      renderSidebarPlaylists();
      if (state.currentView === "playlists") {
        renderPlaylists();
      }
    }
  } catch (err) {
    console.warn("Cloud playlists sync:", err);
  } finally {
    state.isSyncing = false;
  }
}

try {
  listen("auth-success", async (event) => {
    if (event && event.payload) {
      state.auth.isAuthenticated = event.payload.is_authenticated;
      state.auth.storefront = event.payload.storefront;
      state.auth.language = event.payload.language;
      state.auth.userTokenMasked = event.payload.user_token_masked;

      authBtn?.classList.add("authenticated");
      if (authLabel) authLabel.textContent = "Apple ID";
      if (authModal) authModal.style.display = "none";

      renderListenNow();
    }
  });
} catch (err) {
  console.warn("Could not register auth-success listener:", err);
}

// Streamed playback runs inside the hidden player webview. Its CSP blocks loopback
// fetch and Tauri IPC is unavailable on remote origins, so it publishes state through
// the `mk_state` cookie and Rust re-emits it here as `player-state`.
try {
  listen("player-state", (event) => {
    let payload = event?.payload;
    if (typeof payload === "string") {
      try { payload = JSON.parse(payload); } catch (_) { return; }
    }
    if (!payload) return;

    state.streamIsPlaying = !!payload.isPlaying;
    state.streamPosition = payload.position || 0;
    state.streamDuration = payload.duration || 0;
    state.streamingActive = !!payload.id;
    // Re-anchor the local clock so the progress bar stays honest.
    state.streamAnchor = Date.now() - state.streamPosition * 1000;
    state.streamShuffle = !!payload.shuffle;
    state.streamRepeat = payload.repeat || 0;
    state.streamQueueLength = payload.queueLength || 0;
    if (typeof payload.volume === "number") state.streamVolume = payload.volume;

    paintStreamControls();

    if (payload.title) {
      if (nowPlayingTitle) nowPlayingTitle.textContent = payload.title;
      if (nowPlayingArtist) nowPlayingArtist.textContent = payload.artist || "Apple Music";
      const badge = $("#now-playing-badge");
      if (badge) badge.style.display = "block";
    }

    if (payload.artwork) {
      const artImg = $("#now-playing-art");
      const formatted = payload.artwork.replace("{w}", "100").replace("{h}", "100").replace("{f}", "jpg");
      if (artImg) {
        artImg.innerHTML = `<img src="${formatted}" alt="" style="width: 56px; height: 56px; border-radius: 4px; object-fit: cover;" />`;
      }
    }

    const fill = $("#progress-fill");
    if (fill && state.streamDuration > 0) {
      fill.style.width = `${Math.min(100, (state.streamPosition / state.streamDuration) * 100)}%`;
    }

    // Highlight the row that is actually playing (Apple's list state), and clear
    // the highlight from the rest.
    document.querySelectorAll(".track-item.stream-track").forEach((el) => {
      el.classList.toggle("playing", !!payload.id && el.dataset.catalogId === payload.id);
    });
  });
} catch (err) {
  console.warn("Could not register player-state listener:", err);
}

authBtn?.addEventListener("click", () => {
  renderAuthModal();
  authModal.style.display = "flex";
});

modalCloseBtn?.addEventListener("click", () => {
  authModal.style.display = "none";
});

authModal?.addEventListener("click", (e) => {
  if (e.target === authModal) {
    authModal.style.display = "none";
  }
});

function renderAuthModal() {
  if (state.auth.isAuthenticated) {
    modalAuthBody.innerHTML = `
      <div class="modal-status-pill status-active">● Signed In with Apple ID</div>
      <p style="font-size: 13px; color: var(--text-secondary); margin-bottom: 8px;">
        Storefront: <strong>${escapeHtml(state.auth.storefront.toUpperCase())}</strong>
      </p>
      <p style="font-size: 12px; color: var(--text-tertiary); margin-bottom: 20px;">
        Music User Token: ${escapeHtml(state.auth.userTokenMasked || "Active")}
      </p>
      <button class="modal-btn modal-btn-secondary" id="sign-out-btn">Sign Out</button>
    `;
    document.getElementById("sign-out-btn")?.addEventListener("click", async () => {
      try {
        await invoke("sign_out");
        await checkAuthState();
        authModal.style.display = "none";
        renderListenNow();
      } catch (err) {
        console.error("Sign out failed:", err);
      }
    });
  } else {
    modalAuthBody.innerHTML = `
      <div class="modal-status-pill status-inactive">○ Not Signed In</div>
      <p style="font-size: 13px; color: var(--text-secondary); line-height: 1.5; margin-bottom: 16px;">
        Sign in with your Apple ID to access your personal iCloud Music Library, saved playlists, and personalized recommendations.
      </p>
      <button class="modal-btn modal-btn-primary" id="start-sign-in-btn">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
          <path d="M17.05 20.28c-.98.95-2.05.88-3.08.4-1.09-.5-2.08-.48-3.13 0-1.31.62-2 .3-2.77-.4C3.86 15.77 4.5 8.72 8.96 8.26c1.22-.11 2.08.67 3.13.67 1.04 0 1.68-.67 3.13-.67 1.07-.01 2.1.49 2.8 1.24-2.67 1.84-2.2 5.72.39 6.69-.43 1.12-.98 2.22-1.77 3.09h.01zM12.02 8.16c-.22-2.66 2-4.9 4.16-5.16.36 2.92-2.2 5.18-4.16 5.16z"/>
        </svg>
        Sign in with Apple ID (Official Window)
      </button>

      <div style="margin: 20px 0 12px; font-size: 12px; font-weight: 600; color: var(--text-tertiary); text-transform: uppercase;">
        Or Manual Token (Developer / Direct)
      </div>
      <input type="password" class="modal-input" id="manual-token-input" placeholder="Paste Music-User-Token here..." />
      <button class="modal-btn modal-btn-secondary" id="save-token-btn">Save Token</button>
    `;

    document.getElementById("start-sign-in-btn")?.addEventListener("click", async () => {
      try {
        await invoke("start_apple_sign_in");
        authModal.style.display = "none";
      } catch (err) {
        console.error("Start sign in failed:", err);
      }
    });

    document.getElementById("save-token-btn")?.addEventListener("click", async () => {
      const input = document.getElementById("manual-token-input");
      const val = input ? input.value.trim() : "";
      if (val) {
        try {
          await invoke("set_user_session", { token: val });
          await checkAuthState();
          authModal.style.display = "none";
          renderListenNow();
        } catch (err) {
          alert("Failed to save session: " + err);
        }
      }
    });
  }
}

// ─── Search & Catalog Management ───────────────────────────────
searchInput?.addEventListener("input", debounce(async (e) => {
  const query = e.target.value.trim();
  if (query.length >= 2) {
    try {
      const suggestions = await invoke("get_search_suggestions", { query });
      if (suggestions && suggestions.terms.length > 0) {
        searchSuggestions.innerHTML = suggestions.terms
          .slice(0, 6)
          .map(
            (term) => `
          <div class="suggestion-item" data-term="${escapeHtml(term)}">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
              <path d="M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z"/>
            </svg>
            <span>${escapeHtml(term)}</span>
          </div>
        `
          )
          .join("");
        searchSuggestions.style.display = "block";
      } else {
        searchSuggestions.style.display = "none";
      }
    } catch (_err) {
      searchSuggestions.style.display = "none";
    }
  } else {
    searchSuggestions.style.display = "none";
  }
}, 250));

searchSuggestions?.addEventListener("click", (e) => {
  const item = e.target.closest(".suggestion-item");
  if (item) {
    const term = item.dataset.term;
    if (term) {
      searchInput.value = term;
      searchSuggestions.style.display = "none";
      performCatalogSearch(term);
    }
  }
});

document.addEventListener("click", (e) => {
  if (!e.target.closest(".search-container")) {
    searchSuggestions.style.display = "none";
  }
});

searchInput?.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    searchSuggestions.style.display = "none";
    const q = searchInput.value.trim();
    if (q) performCatalogSearch(q);
  }
});

async function performCatalogSearch(term) {
  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">Search</div>
      <div class="view-subtitle">Searching for "${escapeHtml(term)}"...</div>
    </div>
  `;

  try {
    const results = await invoke("search_catalog", { query: term, limit: 25 });
    renderSearchResults(term, results);
  } catch (err) {
    mainView.innerHTML = `
      <div class="view-header">
        <div class="view-title">Search</div>
        <div class="view-subtitle">Error searching: ${escapeHtml(String(err))}</div>
      </div>
    `;
  }
}

function renderSearchResults(term, results) {
  const songs = results.songs || [];
  const albums = results.albums || [];
  const artists = results.artists || [];

  if (songs.length === 0 && albums.length === 0 && artists.length === 0) {
    renderEmpty("No Results", `No results found on Apple Music for "${escapeHtml(term)}".`);
    return;
  }

  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">Results for "${escapeHtml(term)}"</div>
      <div class="view-subtitle">${songs.length} songs · ${albums.length} albums · ${artists.length} artists</div>
    </div>

    ${
      songs.length > 0
        ? `
      <h3 style="margin-bottom: 16px; font-size: 18px; font-weight: 700;">Songs</h3>
      <div class="track-list" style="margin-bottom: 32px;">
        ${songs
          .slice(0, 10)
          .map(
            (song, i) => `
          <div class="track-item stream-track" data-catalog-id="${song.id}" data-stream-url="${escapeHtml(song.preview_url || '')}" data-title="${escapeHtml(song.title)}" data-artist="${escapeHtml(song.artist)}" data-artwork="${escapeHtml(song.artwork_url || '')}" data-duration="${song.duration_ms ? song.duration_ms / 1000 : 30}">
            <div class="track-number">${i + 1}</div>
            <div class="track-info">
              <div class="track-name">${escapeHtml(song.title)}</div>
              <div class="track-artist">${escapeHtml(song.artist)} · ${escapeHtml(song.album)}</div>
            </div>
            <div class="track-quality">
              ${song.has_lossless ? '<span class="quality-badge quality-lossless">Lossless</span>' : '<span class="quality-badge quality-lossless" style="background: rgba(250, 45, 72, 0.2); color: #fa2d48;">AAC</span>'}
              ${song.has_atmos ? '<span class="quality-badge quality-atmos">Atmos</span>' : ""}
            </div>
            <div class="track-duration">${song.duration_ms ? formatTime(song.duration_ms / 1000) : "--:--"}</div>
          </div>
        `
          )
          .join("")}
      </div>
    `
        : ""
    }

    ${
      albums.length > 0
        ? `
      <h3 style="margin-bottom: 16px; font-size: 18px; font-weight: 700;">Albums</h3>
      <div class="album-grid" style="margin-bottom: 32px;">
        ${albums
          .map((a) => {
            const artUrl = a.artwork_url ? a.artwork_url.replace("{w}", "300").replace("{h}", "300").replace("{f}", "jpg") : null;
            return `
            <div class="album-card" data-catalog-album-id="${a.id}">
              <div class="album-card-art">
                ${
                  artUrl
                    ? `<img src="${artUrl}" alt="${escapeHtml(a.title)}" />`
                    : `<div class="placeholder"><svg width="32" height="32" viewBox="0 0 24 24" fill="#555"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 14.5c-2.49 0-4.5-2.01-4.5-4.5S9.51 7.5 12 7.5s4.5 2.01 4.5 4.5-2.01 4.5-4.5 4.5z"/></svg></div>`
                }
              </div>
              <div class="album-card-title">${escapeHtml(a.title)}</div>
              <div class="album-card-artist">${escapeHtml(a.artist)}</div>
            </div>
          `;
          })
          .join("")}
      </div>
    `
        : ""
    }
  `;
}

// ─── Listen Now / Recommendations View ────────────────────────
async function renderListenNow() {
  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">Listen Now</div>
      <div class="view-subtitle">Loading Apple Music catalog...</div>
    </div>
  `;

  try {
    if (state.auth.isAuthenticated) {
      const recs = await invoke("get_recommendations");
      if (recs && recs.length > 0) {
        mainView.innerHTML = `
          <div class="view-header">
            <div class="view-title">Listen Now</div>
            <div class="view-subtitle">Personalized for you</div>
          </div>
          <h3 style="margin-bottom: 16px; font-size: 18px; font-weight: 700;">Made For You</h3>
          <div class="album-grid">
            ${recs
              .map((a) => {
                const artUrl = a.artwork_url ? a.artwork_url.replace("{w}", "300").replace("{h}", "300").replace("{f}", "jpg") : null;
                return `
                <div class="album-card" data-catalog-album-id="${a.id}">
                  <div class="album-card-art">
                    ${artUrl ? `<img src="${artUrl}" alt="${escapeHtml(a.title)}" />` : `<div class="placeholder">...</div>`}
                  </div>
                  <div class="album-card-title">${escapeHtml(a.title)}</div>
                  <div class="album-card-artist">${escapeHtml(a.artist)}</div>
                </div>
              `;
              })
              .join("")}
          </div>
        `;
        return;
      }
    }

    // Default: Top Charts
    const charts = await invoke("get_charts", { limit: 12 });
    mainView.innerHTML = `
      <div class="view-header">
        <div class="view-title">Listen Now</div>
        <div class="view-subtitle">Top Charts · Apple Music</div>
      </div>

      <h3 style="margin-bottom: 16px; font-size: 18px; font-weight: 700;">Top Albums</h3>
      <div class="album-grid" style="margin-bottom: 32px;">
        ${(charts.albums || [])
          .map((a) => {
            const artUrl = a.artwork_url ? a.artwork_url.replace("{w}", "300").replace("{h}", "300").replace("{f}", "jpg") : null;
            return `
            <div class="album-card" data-catalog-album-id="${a.id}">
              <div class="album-card-art">
                ${artUrl ? `<img src="${artUrl}" alt="${escapeHtml(a.title)}" />` : `<div class="placeholder">...</div>`}
              </div>
              <div class="album-card-title">${escapeHtml(a.title)}</div>
              <div class="album-card-artist">${escapeHtml(a.artist)}</div>
            </div>
          `;
          })
          .join("")}
      </div>

      <h3 style="margin-bottom: 16px; font-size: 18px; font-weight: 700;">Top Songs</h3>
      <div class="track-list">
        ${(charts.songs || [])
          .map(
            (s, i) => `
          <div class="track-item stream-track" data-catalog-id="${s.id}" data-stream-url="${escapeHtml(s.preview_url || '')}" data-title="${escapeHtml(s.title)}" data-artist="${escapeHtml(s.artist)}" data-artwork="${escapeHtml(s.artwork_url || '')}" data-duration="${s.duration_ms ? s.duration_ms / 1000 : 30}">
            <div class="track-number">${i + 1}</div>
            <div class="track-info">
              <div class="track-name">${escapeHtml(s.title)}</div>
              <div class="track-artist">${escapeHtml(s.artist)} · ${escapeHtml(s.album)}</div>
            </div>
            <div class="track-quality">
              ${s.has_lossless ? '<span class="quality-badge quality-lossless">Lossless</span>' : '<span class="quality-badge quality-lossless" style="background: rgba(250, 45, 72, 0.2); color: #fa2d48;">AAC</span>'}
            </div>
            <div class="track-duration">${s.duration_ms ? formatTime(s.duration_ms / 1000) : "--:--"}</div>
          </div>
        `
          )
          .join("")}
      </div>
    `;
  } catch (err) {
    mainView.innerHTML = `
      <div class="view-header">
        <div class="view-title">Listen Now</div>
        <div class="view-subtitle">Sign in or scan your local library to start listening.</div>
      </div>
    `;
  }
}

// ─── Browse ───────────────────────────────────────────────────

// Apple's Browse tab is charts and editorial shelves. Charts are the part this
// client can fetch truthfully from the catalog API, so Browse shows those.
async function renderBrowse() {
  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">Browse</div>
      <div class="view-subtitle">Loading charts…</div>
    </div>
  `;

  try {
    const charts = await invoke("get_charts", { types: "songs,albums", limit: 20 });
    const albums = charts.albums || [];
    const songs = charts.songs || [];

    mainView.innerHTML = `
      <div class="view-header">
        <div class="view-title">Browse</div>
        <div class="view-subtitle">Top Charts · Apple Music</div>
      </div>

      <h3 style="margin-bottom: 16px; font-size: 18px; font-weight: 700;">Top Albums</h3>
      <div class="album-grid" style="margin-bottom: 32px;">
        ${albums
          .map((a) => {
            const artUrl = a.artwork_url
              ? a.artwork_url.replace("{w}", "300").replace("{h}", "300").replace("{f}", "jpg")
              : null;
            return `
              <div class="album-card" data-catalog-album-id="${a.id}">
                <div class="album-card-art">
                  ${artUrl ? `<img src="${artUrl}" alt="${escapeHtml(a.title)}" />` : `<div class="placeholder"></div>`}
                </div>
                <div class="album-card-title">${escapeHtml(a.title)}</div>
                <div class="album-card-artist">${escapeHtml(a.artist)}</div>
              </div>
            `;
          })
          .join("")}
      </div>

      <h3 style="margin-bottom: 16px; font-size: 18px; font-weight: 700;">Top Songs</h3>
      <div class="track-list">
        ${songs.map((s, i) => renderCloudTrackRow(s, i + 1)).join("")}
      </div>
    `;
  } catch (err) {
    mainView.innerHTML = `
      <div class="view-header">
        <div class="view-title">Browse</div>
        <div class="view-subtitle">Could not load charts: ${escapeHtml(String(err))}</div>
      </div>
    `;
  }
}

// Apple Music Radio is served by Apple's separate stations API. Until that is
// wired up, say so instead of pretending a Radio page exists.
function renderRadio() {
  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">Radio</div>
      <div class="view-subtitle">Apple Music Radio</div>
    </div>
    <div class="empty-state">
      <div class="empty-state-title">Radio is not wired up yet</div>
      <div class="empty-state-desc">
        Live stations need Apple's stations API. In the meantime, Listen Now and Browse play
        the same catalog, and your iCloud library is under Library.
      </div>
    </div>
  `;
}

// ─── Library Commands ──────────────────────────────────────────
async function scanLibrary() {
  try {
    const count = await invoke("scan_library");
    trackCount.textContent = count;
    await loadLibrary();
    if (state.currentView === "albums") {
      renderAlbums();
    } else if (state.currentView === "artists") {
      renderArtists();
    } else if (state.currentView === "playlists") {
      renderPlaylists();
    } else {
      renderLibrary();
    }
  } catch (err) {
    console.error("Scan failed:", err);
  }
}

async function loadLibrary() {
  try {
    state.tracks = await invoke("get_tracks");
    state.albums = await invoke("get_albums");
    state.artists = await invoke("get_artists");
    state.playlists = await invoke("get_playlists");
    trackCount.textContent = state.tracks.length;
    renderSidebarPlaylists();
  } catch (err) {
    console.error("Load library failed:", err);
  }
}

function renderSidebarPlaylists() {
  if (!playlistList) return;
  playlistList.innerHTML = `
    <div class="playlist-item default-liked">Liked Songs</div>
    ${state.playlists
      .map(
        (p) => `
      <div class="playlist-item" data-playlist-id="${p.id}">
        ${escapeHtml(p.name)}
      </div>
    `
      )
      .join("")}
    ${
      state.cloudPlaylists && state.cloudPlaylists.length > 0
        ? `
      <div style="font-size: 11px; font-weight: 700; color: var(--text-tertiary); margin: 16px 12px 6px; text-transform: uppercase; letter-spacing: 0.5px;">Apple Music Playlists</div>
      ${state.cloudPlaylists
        .map(
          (cp) => `
        <div class="playlist-item cloud-playlist" data-cloud-playlist-id="${cp.id}">
          <span style="font-size: 12px; margin-right: 4px;">☁️</span> ${escapeHtml(cp.name)}
        </div>
      `
        )
        .join("")}
    `
        : ""
    }
    <div class="playlist-item create-playlist" id="create-playlist">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z"/></svg>
      <span>New Playlist</span>
    </div>
  `;

  document.getElementById("create-playlist")?.addEventListener("click", handleCreatePlaylist);
}

async function handleCreatePlaylist() {
  const name = prompt("Enter new playlist name:");
  if (name && name.trim()) {
    try {
      await invoke("create_playlist", { name: name.trim(), description: "" });
      await loadLibrary();
      if (state.currentView === "playlists") renderPlaylists();
    } catch (err) {
      console.error("Failed to create playlist:", err);
    }
  }
}

// ─── View Renderers ────────────────────────────────────────────

function renderWelcome() {
  const signedIn = state.auth.isAuthenticated;
  mainView.innerHTML = `
    <div class="welcome-view">
      <svg class="welcome-icon" viewBox="0 0 48 48" fill="currentColor">
        <path d="M24 6C14.06 6 6 14.06 6 24s8.06 18 18 18 18-8.06 18-18S33.94 6 24 6zm-4 26.5v-17l12 8.5-12 8.5z"/>
      </svg>
      <div class="welcome-title">${signedIn ? "You're signed in" : "Welcome to Apple Music"}</div>
      <div class="welcome-desc">
        ${
          signedIn
            ? "Browse Listen Now, search the catalog, or open your iCloud library from the sidebar."
            : "Sign in with your Apple ID to use your existing Apple Music subscription. No developer account needed."
        }
      </div>
      ${
        signedIn
          ? '<button class="welcome-action" id="welcome-listen-btn">Go to Listen Now</button>'
          : '<button class="welcome-action" id="welcome-signin-btn">Sign In</button>'
      }
    </div>
  `;
  document.getElementById("welcome-signin-btn")?.addEventListener("click", () => authBtn?.click());
  document.getElementById("welcome-listen-btn")?.addEventListener("click", () => {
    document.querySelector('.nav-item[data-view="listen-now"]')?.click();
  });
}

// ─── iCloud library (the user's own Apple Music library) ────────

function renderSignInPrompt(title) {
  mainView.innerHTML = `
    <div class="view-header"><div class="view-title">${escapeHtml(title)}</div></div>
    <div class="empty-state">
      <svg class="empty-state-icon" width="64" height="64" viewBox="0 0 48 48" fill="currentColor">
        <path d="M24 6C14.06 6 6 14.06 6 24s8.06 18 18 18 18-8.06 18-18S33.94 6 24 6zm-4 26.5v-17l12 8.5-12 8.5z"/>
      </svg>
      <div class="empty-state-title">Sign in to Apple Music</div>
      <div class="empty-state-desc">Your library, albums and artists come from your Apple Music account.</div>
      <button id="library-signin-btn">Sign In</button>
    </div>
  `;
  document.getElementById("library-signin-btn")?.addEventListener("click", () => authBtn?.click());
}

async function ensureCloudLibrary(force = false) {
  if (!state.auth.isAuthenticated) return [];
  if (!force && state.cloudTracks.length > 0) return state.cloudTracks;
  try {
    const songs = await invoke("get_personal_library");
    state.cloudTracks = Array.isArray(songs) ? songs : [];
  } catch (err) {
    console.warn("iCloud library load failed:", err);
    state.cloudTracks = [];
  }
  if (trackCount) trackCount.textContent = String(state.cloudTracks.length);
  return state.cloudTracks;
}

// Group iCloud songs into albums and artists client-side — Apple's library views
// are derived from the song list, and we already have everything needed.
function deriveLibraryAlbums(songs) {
  const map = new Map();
  songs.forEach((s) => {
    const key = `${s.artist}||${s.album || "Unknown Album"}`;
    if (!map.has(key)) {
      map.set(key, {
        key,
        title: s.album || "Unknown Album",
        artist: s.artist,
        artwork_url: s.artwork_url,
        tracks: [],
        year: s.release_date ? s.release_date.slice(0, 4) : "",
      });
    }
    map.get(key).tracks.push(s);
  });
  return Array.from(map.values());
}

function deriveLibraryArtists(songs) {
  const map = new Map();
  songs.forEach((s) => {
    if (!map.has(s.artist)) map.set(s.artist, { name: s.artist, tracks: [], artwork_url: s.artwork_url });
    const a = map.get(s.artist);
    a.tracks.push(s);
    if (!a.artwork_url) a.artwork_url = s.artwork_url;
  });
  return Array.from(map.values());
}

async function renderLibrary() {
  if (!state.auth.isAuthenticated) return renderSignInPrompt("Library");

  const songs = await ensureCloudLibrary();
  if (songs.length === 0) {
    mainView.innerHTML = `
      <div class="view-header">
        <div class="view-title">Library</div>
        <div class="view-subtitle">0 songs · iCloud</div>
      </div>
      <div class="empty-state">
        <div class="empty-state-title">Your iCloud library is empty</div>
        <div class="empty-state-desc">Add songs in Apple Music and they appear here.</div>
      </div>
    `;
    return;
  }

  const albums = deriveLibraryAlbums(songs).slice(0, 12);
  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">Library</div>
      <div class="view-subtitle">${songs.length} songs · ${albums.length} albums · iCloud</div>
    </div>

    <h3 style="margin-bottom: 16px; font-size: 18px; font-weight: 700;">Albums</h3>
    <div class="album-grid" style="margin-bottom: 32px;">
      ${albums.map((a) => renderLibraryAlbumCard(a)).join("")}
    </div>

    <h3 style="margin: 0 0 16px; font-size: 18px; font-weight: 700;">Songs</h3>
    <div class="track-list">
      ${songs.map((s, i) => renderCloudTrackRow(s, i + 1)).join("")}
    </div>
  `;
}

async function renderAlbums() {
  if (!state.auth.isAuthenticated) return renderSignInPrompt("Albums");

  const songs = await ensureCloudLibrary();
  if (songs.length === 0) {
    return renderEmpty("No Albums", "Your iCloud library has no songs yet.");
  }
  const albums = deriveLibraryAlbums(songs);
  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">Albums</div>
      <div class="view-subtitle">${albums.length} albums · iCloud</div>
    </div>
    <div class="album-grid">
      ${albums.map((a) => renderLibraryAlbumCard(a)).join("")}
    </div>
  `;
}

async function renderArtists() {
  if (!state.auth.isAuthenticated) return renderSignInPrompt("Artists");

  const songs = await ensureCloudLibrary();
  if (songs.length === 0) {
    return renderEmpty("No Artists", "Your iCloud library has no songs yet.");
  }
  const artists = deriveLibraryArtists(songs);
  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">Artists</div>
      <div class="view-subtitle">${artists.length} artists · iCloud</div>
    </div>
    <div class="artist-grid">
      ${artists
        .map(
          (artist) => `
        <div class="artist-card" data-library-artist="${escapeHtml(artist.name)}">
          <div class="artist-avatar">
            <svg width="36" height="36" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/>
            </svg>
          </div>
          <div class="artist-name">${escapeHtml(artist.name)}</div>
          <div class="artist-meta">${artist.tracks.length} songs</div>
        </div>
      `
        )
        .join("")}
    </div>
  `;
}

function renderPlaylists() {
  const hasLocal = state.playlists.length > 0;
  const hasCloud = state.cloudPlaylists && state.cloudPlaylists.length > 0;

  if (!hasLocal && !hasCloud) {
    mainView.innerHTML = `
      <div class="view-header" style="display: flex; justify-content: space-between; align-items: center;">
        <div class="view-title">Playlists</div>
        ${state.auth.isAuthenticated ? '<button class="welcome-action" id="sync-cloud-btn" style="margin: 0; padding: 8px 16px;">☁️ Sync iCloud</button>' : ''}
      </div>
      <div class="empty-state">
        <svg class="empty-state-icon" width="64" height="64" viewBox="0 0 48 48" fill="currentColor">
          <path d="M15 6H3v2h12V6zm0 4H3v2h12v-2zM3 16h8v-2H3v2zM17 6v8.18c-.31-.11-.65-.18-1-.18-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3V8h3V6h-5z"/>
        </svg>
        <div class="empty-state-title">No Playlists Yet</div>
        <div class="empty-state-desc">Create your own playlist or sign in to sync your Apple Music iCloud playlists.</div>
        <button id="create-first-playlist-btn">Create Playlist</button>
      </div>
    `;
    document.getElementById("create-first-playlist-btn")?.addEventListener("click", handleCreatePlaylist);
    document.getElementById("sync-cloud-btn")?.addEventListener("click", syncCloudPlaylists);
    return;
  }

  mainView.innerHTML = `
    <div class="view-header" style="display: flex; justify-content: space-between; align-items: center;">
      <div>
        <div class="view-title">Playlists</div>
        <div class="view-subtitle">${state.playlists.length} local · ${state.cloudPlaylists.length} iCloud</div>
      </div>
      <div style="display: flex; gap: 10px;">
        ${state.auth.isAuthenticated ? '<button class="welcome-action" id="sync-cloud-top-btn" style="margin: 0; padding: 8px 16px;">☁️ Sync iCloud</button>' : ''}
        <button class="welcome-action" id="new-playlist-top-btn" style="margin: 0; padding: 8px 16px;">+ New Playlist</button>
      </div>
    </div>

    ${
      hasCloud
        ? `
      <h3 style="margin: 20px 0 16px; font-size: 18px; font-weight: 700;">iCloud Playlists (Apple Music)</h3>
      <div class="album-grid" style="margin-bottom: 32px;">
        ${state.cloudPlaylists
          .map((cp) => {
            const artUrl = cp.artwork_url ? cp.artwork_url.replace("{w}", "300").replace("{h}", "300").replace("{f}", "jpg") : null;
            return `
            <div class="album-card cloud-playlist-card" data-cloud-playlist-id="${cp.id}">
              <div class="album-card-art">
                ${
                  artUrl
                    ? `<img src="${artUrl}" alt="${escapeHtml(cp.name)}" />`
                    : `
                  <div class="placeholder">
                    <svg width="36" height="36" viewBox="0 0 24 24" fill="#fa2d48"><path d="M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96zM19 18H6c-2.21 0-4-1.79-4-4 0-2.05 1.53-3.76 3.56-3.97l1.07-.11.5-.95C8.08 7.14 9.94 6 12 6c2.62 0 4.88 1.86 5.39 4.43l.3 1.5 1.53.11c1.56.1 2.78 1.41 2.78 2.96 0 1.65-1.35 3-3 3z"/></svg>
                  </div>
                `
                }
              </div>
              <div class="album-card-title">${escapeHtml(cp.name)}</div>
              <div class="album-card-artist"><span class="quality-badge" style="margin-right: 4px; font-size: 10px; background: rgba(250, 45, 72, 0.2); color: #fa2d48;">iCloud</span> ${cp.track_count || cp.tracks.length} songs</div>
            </div>
          `;
          })
          .join("")}
      </div>
    `
        : ""
    }

    ${
      hasLocal
        ? `
      <h3 style="margin: 20px 0 16px; font-size: 18px; font-weight: 700;">Local Playlists</h3>
      <div class="album-grid">
        ${state.playlists
          .map(
            (p) => `
          <div class="album-card" data-playlist-id="${p.id}">
            <div class="album-card-art">
              <div class="placeholder">
                <svg width="36" height="36" viewBox="0 0 24 24" fill="#555">
                  <path d="M15 6H3v2h12V6zm0 4H3v2h12v-2zM3 16h8v-2H3v2zM17 6v8.18c-.31-.11-.65-.18-1-.18-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3V8h3V6h-5z"/>
                </svg>
              </div>
            </div>
            <div class="album-card-title">${escapeHtml(p.name)}</div>
            <div class="album-card-artist">${p.tracks.length} songs</div>
          </div>
        `
          )
          .join("")}
      </div>
    `
        : ""
    }
  `;
  document.getElementById("new-playlist-top-btn")?.addEventListener("click", handleCreatePlaylist);
  document.getElementById("sync-cloud-top-btn")?.addEventListener("click", syncCloudPlaylists);
}

async function showCloudPlaylistDetail(playlistId) {
  const playlist = state.cloudPlaylists.find((p) => p.id === playlistId);
  const name = playlist ? playlist.name : "Cloud Playlist";
  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">${escapeHtml(name)}</div>
      <div class="view-subtitle">Loading tracks from Apple Music...</div>
    </div>
  `;

  try {
    let tracks = playlist && playlist.tracks && playlist.tracks.length > 0 ? playlist.tracks : [];
    if (tracks.length === 0) {
      tracks = await invoke("get_cloud_playlist_tracks", { playlistId });
    }

    if (tracks.length === 0) {
      mainView.innerHTML = `
        <div class="view-header">
          <div class="view-title">${escapeHtml(name)}</div>
          <div class="view-subtitle">0 tracks</div>
        </div>
        <div class="empty-state">
          <div class="empty-state-title">Empty Playlist</div>
          <div class="empty-state-desc">This playlist has no tracks.</div>
        </div>
      `;
      return;
    }

    mainView.innerHTML = `
      <div class="view-header">
        <div class="view-title">${escapeHtml(name)}</div>
        <div class="view-subtitle">${tracks.length} songs · Apple Music Cloud</div>
      </div>
      <div class="track-list">
        ${tracks
          .map(
            (song, i) => `
          <div class="track-item stream-track" data-catalog-id="${song.id}" data-stream-url="${escapeHtml(song.preview_url || '')}" data-title="${escapeHtml(song.title)}" data-artist="${escapeHtml(song.artist)}" data-artwork="${escapeHtml(song.artwork_url || '')}" data-duration="${song.duration_ms ? song.duration_ms / 1000 : 30}">
            <div class="track-number">${i + 1}</div>
            <div class="track-info">
              <div class="track-name">${escapeHtml(song.title)}</div>
              <div class="track-artist">${escapeHtml(song.artist)} · ${escapeHtml(song.album)}</div>
            </div>
            <div class="track-quality">
              ${song.has_lossless ? '<span class="quality-badge quality-lossless">Lossless</span>' : '<span class="quality-badge quality-lossless" style="background: rgba(250, 45, 72, 0.2); color: #fa2d48;">AAC</span>'}
            </div>
            <div class="track-duration">${song.duration_ms ? formatTime(song.duration_ms / 1000) : "--:--"}</div>
          </div>
        `
          )
          .join("")}
      </div>
    `;
  } catch (err) {
    mainView.innerHTML = `
      <div class="view-header">
        <div class="view-title">${escapeHtml(name)}</div>
        <div class="view-subtitle">Error loading tracks: ${escapeHtml(String(err))}</div>
      </div>
    `;
  }
}

function showLocalPlaylistDetail(playlistId) {
  const playlist = state.playlists.find((p) => p.id === playlistId);
  if (!playlist) return;

  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">${escapeHtml(playlist.name)}</div>
      <div class="view-subtitle">${playlist.tracks.length} songs · Local Playlist</div>
    </div>
    <div class="track-list">
      ${playlist.tracks
        .map(
          (track, i) => `
        <div class="track-item" data-track-id="${track.id}">
          <div class="track-number">${i + 1}</div>
          <div class="track-info">
            <div class="track-name">${escapeHtml(track.title)}</div>
            <div class="track-artist">${escapeHtml(track.artist)} · ${escapeHtml(track.album)}</div>
          </div>
          <div class="track-duration">${formatTime(track.duration_secs)}</div>
        </div>
      `
        )
        .join("")}
    </div>
  `;
}

function renderEmpty(title, desc) {
  mainView.innerHTML = `
    <div class="empty-state">
      <svg class="empty-state-icon" width="64" height="64" viewBox="0 0 48 48" fill="currentColor">
        <path d="M24 6C14.06 6 6 14.06 6 24s8.06 18 18 18 18-8.06 18-18S33.94 6 24 6zm-4 26.5v-17l12 8.5-12 8.5z"/>
      </svg>
      <div class="empty-state-title">${title}</div>
      <div class="empty-state-desc">${desc}</div>
    </div>
  `;
}

// ─── Card & Row Renderers ──────────────────────────────────────

function renderAlbumCard(album) {
  const artHtml = album.art_path
    ? `<img src="${convertFileSrc(album.art_path)}" alt="${escapeHtml(album.title)}" onerror="this.style.display='none'" />`
    : `<div class="placeholder">
        <svg width="32" height="32" viewBox="0 0 24 24" fill="#555">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 14.5c-2.49 0-4.5-2.01-4.5-4.5S9.51 7.5 12 7.5s4.5 2.01 4.5 4.5-2.01 4.5-4.5 4.5z"/>
        </svg>
      </div>`;

  return `
    <div class="album-card" data-album-id="${album.id}">
      <div class="album-card-art">
        ${artHtml}
        <div class="play-overlay" title="Play Album">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="white">
            <path d="M8 5v14l11-7z"/>
          </svg>
        </div>
      </div>
      <div class="album-card-title">${escapeHtml(album.title)}</div>
      <div class="album-card-artist">${escapeHtml(album.artist)}</div>
    </div>
  `;
}

function renderTrackRow(track, index) {
  const isPlayingCurrent = state.playback.currentTrackId === track.id;
  const hasLossless =
    track.file_path.endsWith(".flac") ||
    track.file_path.endsWith(".alac") ||
    track.file_path.endsWith(".wav");

  const duration = track.duration_secs > 0 ? formatTime(track.duration_secs) : "--:--";

  return `
    <div class="track-item ${isPlayingCurrent ? "playing" : ""}" data-track-id="${track.id}">
      <div class="track-number">${index}</div>
      <div class="track-info">
        <div class="track-name">${escapeHtml(track.title)}</div>
        <div class="track-artist">${escapeHtml(track.artist)} · ${escapeHtml(track.album)}</div>
      </div>
      <div class="track-quality">
        ${hasLossless ? '<span class="quality-badge quality-lossless">Lossless</span>' : ""}
      </div>
      <div class="track-duration">${duration}</div>
    </div>
  `;
}

// A row for any Apple Music song (catalog or iCloud). The data attributes are what
// the delegated click handler reads to build the MusicKit queue.
function renderCloudTrackRow(song, index) {
  const duration = song.duration_ms ? formatTime(song.duration_ms / 1000) : "--:--";
  const quality = song.has_lossless
    ? '<span class="quality-badge quality-lossless">Lossless</span>'
    : '<span class="quality-badge quality-lossless" style="background: rgba(250, 45, 72, 0.2); color: #fa2d48;">AAC</span>';
  const atmos = song.has_atmos ? '<span class="quality-badge quality-atmos">Atmos</span>' : "";

  return `
    <div class="track-item stream-track" data-catalog-id="${song.id}" data-stream-url="${escapeHtml(song.preview_url || "")}" data-title="${escapeHtml(song.title)}" data-artist="${escapeHtml(song.artist)}" data-artwork="${escapeHtml(song.artwork_url || "")}" data-duration="${song.duration_ms ? song.duration_ms / 1000 : 0}">
      <div class="track-number">${index}</div>
      <div class="track-info">
        <div class="track-name">${escapeHtml(song.title)}</div>
        <div class="track-artist">${escapeHtml(song.artist)}${song.album ? " · " + escapeHtml(song.album) : ""}</div>
      </div>
      <div class="track-quality">${quality}${atmos}</div>
      <div class="track-duration">${duration}</div>
    </div>
  `;
}

function renderLibraryAlbumCard(group) {
  const artUrl = group.artwork_url
    ? group.artwork_url.replace("{w}", "300").replace("{h}", "300").replace("{f}", "jpg")
    : null;
  return `
    <div class="album-card" data-library-album="${escapeHtml(group.key)}">
      <div class="album-card-art">
        ${
          artUrl
            ? `<img src="${artUrl}" alt="${escapeHtml(group.title)}" />`
            : `<div class="placeholder"><svg width="32" height="32" viewBox="0 0 24 24" fill="#555"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 14.5c-2.49 0-4.5-2.01-4.5-4.5S9.51 7.5 12 7.5s4.5 2.01 4.5 4.5-2.01 4.5-4.5 4.5z"/></svg></div>`
        }
      </div>
      <div class="album-card-title">${escapeHtml(group.title)}</div>
      <div class="album-card-artist">${escapeHtml(group.artist)}</div>
    </div>
  `;
}

// ─── Play a list of Apple Music songs ──────────────────────────

function optimisticallyPlay(song, queueLength) {
  if (!song) return;
  if (nowPlayingTitle) nowPlayingTitle.textContent = song.title || "Apple Music";
  if (nowPlayingArtist) nowPlayingArtist.textContent = song.artist || "Apple Music";
  const badge = $("#now-playing-badge");
  if (badge) badge.style.display = "block";
  if (song.artwork_url) {
    const artImg = $("#now-playing-art");
    const formatted = song.artwork_url.replace("{w}", "100").replace("{h}", "100").replace("{f}", "jpg");
    if (artImg) {
      artImg.innerHTML = `<img src="${formatted}" alt="" style="width: 56px; height: 56px; border-radius: 4px; object-fit: cover;" />`;
    }
  }
  state.streamingActive = true;
  state.streamIsPlaying = true;
  state.streamPosition = 0;
  state.streamAnchor = Date.now();
  state.streamDuration = song.duration_ms ? song.duration_ms / 1000 : 0;
  state.streamQueueLength = queueLength || 1;
  if (progressFill) progressFill.style.width = "0%";
  if (currentTime) currentTime.textContent = "0:00";
  if (totalTime && state.streamDuration > 0) totalTime.textContent = formatTime(state.streamDuration);
}

async function playCatalogTracks(tracks, startId, shuffle = false) {
  const ids = tracks.map((t) => t.id).filter(Boolean);
  if (ids.length === 0) return;
  const start = startId && ids.includes(startId) ? startId : ids[0];
  const song = tracks.find((t) => t.id === start) || tracks[0];

  optimisticallyPlay(song, ids.length);
  try {
    await invoke("stream_play", { songId: start, queue: ids, shuffle });
  } catch (err) {
    console.error("Stream play failed:", err);
    state.streamingActive = false;
    state.streamIsPlaying = false;
  }
}

// ─── Catalog album detail ──────────────────────────────────────

async function showAlbumDetail(albumId) {
  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">Album</div>
      <div class="view-subtitle">Loading album…</div>
    </div>
  `;
  try {
    const album = await invoke("get_catalog_album", { albumId });
    renderAlbumDetail(album);
  } catch (err) {
    mainView.innerHTML = `
      <div class="view-header">
        <div class="view-title">Album</div>
        <div class="view-subtitle">Could not load this album: ${escapeHtml(String(err))}</div>
      </div>
    `;
  }
}

function renderAlbumDetail(album) {
  const tracks = (album.tracks || [])
    .slice()
    .sort((a, b) => (a.track_number || 0) - (b.track_number || 0));
  const artUrl = album.artwork_url
    ? album.artwork_url.replace("{w}", "400").replace("{h}", "400").replace("{f}", "jpg")
    : null;
  const year = album.release_date ? album.release_date.slice(0, 4) : "";
  const totalSecs = tracks.reduce((sum, t) => sum + (t.duration_ms || 0) / 1000, 0);
  const meta = [year, `${tracks.length || album.track_count} songs`, totalSecs > 0 ? formatTime(totalSecs) : ""]
    .filter(Boolean)
    .join(" · ");

  mainView.innerHTML = `
    <div class="album-detail">
      <div class="album-detail-art">
        ${artUrl ? `<img src="${artUrl}" alt="${escapeHtml(album.title)}" />` : ""}
      </div>
      <div class="album-detail-meta">
        <div class="album-detail-kind">Album</div>
        <h1 class="album-detail-title">${escapeHtml(album.title)}</h1>
        <div class="album-detail-artist">${escapeHtml(album.artist)}</div>
        <div class="album-detail-sub">${escapeHtml(meta)}</div>
        <div class="album-detail-actions">
          <button class="play-pill" id="play-album-btn">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>
            Play
          </button>
          <button class="play-pill secondary" id="shuffle-album-btn">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M10.59 9.17L5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41l-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z"/></svg>
            Shuffle
          </button>
        </div>
      </div>
    </div>
    <div class="track-list">
      ${tracks.map((t, i) => renderCloudTrackRow(t, i + 1)).join("")}
    </div>
  `;

  document.getElementById("play-album-btn")?.addEventListener("click", () => {
    playCatalogTracks(tracks, tracks[0]?.id, false);
  });
  document.getElementById("shuffle-album-btn")?.addEventListener("click", () => {
    const pick = tracks[Math.floor(Math.random() * tracks.length)];
    playCatalogTracks(tracks, pick?.id, true);
  });
}

// ─── iCloud album / artist detail (derived from the song list) ─

function showLibraryAlbumDetail(key) {
  const all = ensureCloudLibraryCached();
  const group = deriveLibraryAlbums(all).find((g) => g.key === key);
  if (!group) return;
  const tracks = group.tracks.slice();
  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">${escapeHtml(group.title)}</div>
      <div class="view-subtitle">${escapeHtml(group.artist)} · ${tracks.length} songs · iCloud</div>
    </div>
    <div class="album-detail-actions" style="margin-bottom: 20px;">
      <button class="play-pill" id="play-lib-album-btn">Play</button>
      <button class="play-pill secondary" id="shuffle-lib-album-btn">Shuffle</button>
    </div>
    <div class="track-list">
      ${tracks.map((t, i) => renderCloudTrackRow(t, i + 1)).join("")}
    </div>
  `;
  document.getElementById("play-lib-album-btn")?.addEventListener("click", () => {
    playCatalogTracks(tracks, tracks[0]?.id, false);
  });
  document.getElementById("shuffle-lib-album-btn")?.addEventListener("click", () => {
    const pick = tracks[Math.floor(Math.random() * tracks.length)];
    playCatalogTracks(tracks, pick?.id, true);
  });
}

function showArtistDetail(name) {
  const all = ensureCloudLibraryCached();
  const tracks = all.filter((s) => s.artist === name);
  if (tracks.length === 0) return;
  mainView.innerHTML = `
    <div class="view-header">
      <div class="view-title">${escapeHtml(name)}</div>
      <div class="view-subtitle">${tracks.length} songs · iCloud library</div>
    </div>
    <div class="album-detail-actions" style="margin-bottom: 20px;">
      <button class="play-pill" id="play-artist-btn">Play</button>
    </div>
    <div class="track-list">
      ${tracks.map((t, i) => renderCloudTrackRow(t, i + 1)).join("")}
    </div>
  `;
  document.getElementById("play-artist-btn")?.addEventListener("click", () => {
    playCatalogTracks(tracks, tracks[0]?.id, false);
  });
}

// The library detail views render from the already-loaded list; if the user opens
// them before a load finished, fall back to whatever is cached (possibly empty).
function ensureCloudLibraryCached() {
  return state.cloudTracks || [];
}

// ─── Playback Delegated Events (Tracks & Albums & Playlists) ───────────

// The list the user clicked in becomes MusicKit's queue, so next/previous behave
// like the real app (they move through the album/search result/playlist you see).
function catalogQueueFor(trackItem) {
  const container = trackItem.closest(".track-list") || trackItem.parentElement;
  if (!container) return [];
  return Array.from(container.querySelectorAll(".track-item[data-catalog-id]"))
    .map((el) => el.dataset.catalogId)
    .filter(Boolean);
}

document.addEventListener("click", async (e) => {
  // 1. Cloud Playlist Item Click
  const cloudItem = e.target.closest("[data-cloud-playlist-id]");
  if (cloudItem && !e.target.closest(".track-item")) {
    const cpId = cloudItem.dataset.cloudPlaylistId;
    if (cpId) {
      showCloudPlaylistDetail(cpId);
      return;
    }
  }

  // 2. Local Playlist Item Click
  const localPlaylistItem = e.target.closest(".album-card[data-playlist-id], .playlist-item[data-playlist-id]");
  if (localPlaylistItem && !e.target.closest(".track-item")) {
    const pId = localPlaylistItem.dataset.playlistId;
    if (pId) {
      showLocalPlaylistDetail(pId);
      return;
    }
  }

  // 2b. Catalog album card → album detail view
  const catalogAlbumCard = e.target.closest(".album-card[data-catalog-album-id]");
  if (catalogAlbumCard && !e.target.closest(".play-overlay")) {
    const id = catalogAlbumCard.dataset.catalogAlbumId;
    if (id) {
      showAlbumDetail(id);
      return;
    }
  }

  // 2c. iCloud library album card → derived album detail
  const libraryAlbumCard = e.target.closest(".album-card[data-library-album]");
  if (libraryAlbumCard && !e.target.closest(".play-overlay")) {
    showLibraryAlbumDetail(libraryAlbumCard.dataset.libraryAlbum);
    return;
  }

  // 2d. iCloud artist card → that artist's songs
  const artistCard = e.target.closest(".artist-card[data-library-artist]");
  if (artistCard) {
    showArtistDetail(artistCard.dataset.libraryArtist);
    return;
  }

  // 3. Play Album Overlay Click
  const albumOverlay = e.target.closest(".play-overlay");
  const albumCard = e.target.closest(".album-card");
  if (albumOverlay && albumCard) {
    e.stopPropagation();
    if (albumCard.dataset.catalogAlbumId) {
      try {
        const album = await invoke("get_catalog_album", { albumId: albumCard.dataset.catalogAlbumId });
        const tracks = album.tracks || [];
        if (tracks.length > 0) await playCatalogTracks(tracks, tracks[0].id, false);
      } catch (err) {
        console.error("Play album failed:", err);
      }
      return;
    }
    const albumId = albumCard.dataset.albumId;
    if (albumId) {
      try {
        await invoke("play_album", { albumId });
        await syncPlaybackStatus();
      } catch (err) {
        console.error("Play album failed:", err);
      }
    }
    return;
  }

  // 4. Play Track Click (Stream Online or Local File)
  const trackItem = e.target.closest(".track-item");
  if (trackItem) {
    // Stream track (Apple Music Online)
    const streamUrl = trackItem.dataset.streamUrl;
    const catalogId = trackItem.dataset.catalogId;
    if (streamUrl || catalogId) {
      const title = trackItem.dataset.title || trackItem.querySelector(".track-name")?.textContent || "Apple Music Stream";
      const artist = trackItem.dataset.artist || trackItem.querySelector(".track-artist")?.textContent || "Apple Music";
      const durationSecs = parseFloat(trackItem.dataset.duration) || 30.0;
      const artwork = trackItem.dataset.artwork;

      if (nowPlayingTitle) nowPlayingTitle.textContent = title;
      if (nowPlayingArtist) nowPlayingArtist.textContent = artist;
      const badge = $("#now-playing-badge");
      if (badge) badge.style.display = "block";
      if (artwork) {
        const artImg = $("#now-playing-art");
        const formattedArt = artwork.replace("{w}", "100").replace("{h}", "100").replace("{f}", "jpg");
        if (artImg) {
          artImg.innerHTML = `<img src="${formattedArt}" alt="${escapeHtml(title)}" style="width: 56px; height: 56px; border-radius: 4px; object-fit: cover;" />`;
        }
      }

      try {
        if (catalogId) {
          // Full song via Apple's own player (PlayReady), not the 30s preview.
          // The rows around this one become the queue, so next/previous work.
          const queue = catalogQueueFor(trackItem);
          optimisticallyPlay(
            { title, artist, artwork_url: artwork, duration_ms: durationSecs * 1000 },
            queue.length || 1
          );
          await invoke("stream_play", { songId: catalogId, queue, shuffle: null });
        } else {
          await invoke("play_stream_track", {
            trackId: "stream_" + Date.now(),
            streamUrl: streamUrl || "",
            title,
            artist,
            durationSecs,
          });
          await syncPlaybackStatus();
        }
      } catch (err) {
        console.error("Play stream track failed:", err);
        state.streamingActive = false;
        state.streamIsPlaying = false;
      }
      return;
    }

    // Local Track
    const trackId = trackItem.dataset.trackId;
    if (trackId) {
      try {
        await invoke("play_track", { trackId });
        const badge = $("#now-playing-badge");
        if (badge) badge.style.display = "none";
        await syncPlaybackStatus();
      } catch (err) {
        console.error("Play track failed:", err);
      }
    }
  }
});

// ─── Playback Controls ─────────────────────────────────────────

// Repaint shuffle/repeat/volume from the hidden player's own state, so the
// buttons show what Apple Music is doing rather than a local guess.
function paintStreamControls() {
  const shuffleOn = state.streamShuffle;
  if (shuffleBtn) {
    shuffleBtn.classList.toggle("active", shuffleOn);
    shuffleBtn.style.color = shuffleOn ? "var(--accent)" : "";
  }

  const mode = state.streamRepeat;
  const label = mode === 2 ? "All" : mode === 1 ? "One" : "Off";
  if (repeatBtn) {
    repeatBtn.classList.toggle("active", mode !== 0);
    repeatBtn.style.color = mode !== 0 ? "var(--accent)" : "";
    repeatBtn.setAttribute("title", `Repeat: ${label}`);
  }

  if (typeof state.streamVolume === "number" && volumeFill) {
    volumeFill.style.width = `${state.streamVolume * 100}%`;
  }
}

playBtn?.addEventListener("click", async () => {
  try {
    // Streaming (hidden Apple player) takes priority when it owns playback.
    if (state.streamingActive) {
      if (state.streamIsPlaying) {
        state.streamPosition = Math.min(
          state.streamDuration || 0,
          (Date.now() - state.streamAnchor) / 1000
        );
        state.streamIsPlaying = false;
        await invoke("stream_pause");
      } else {
        state.streamAnchor = Date.now() - (state.streamPosition || 0) * 1000;
        state.streamIsPlaying = true;
        await invoke("stream_resume");
      }
      return;
    }
    if (state.playback.isPlaying) {
      await invoke("pause_playback");
    } else {
      if (state.playback.currentTrackId) {
        await invoke("resume_playback");
      } else if (state.tracks.length > 0) {
        await invoke("play_track", { trackId: state.tracks[0].id });
      }
    }
    await syncPlaybackStatus();
  } catch (err) {
    console.error("Play/Pause failed:", err);
  }
});

prevBtn?.addEventListener("click", async () => {
  try {
    if (state.streamingActive) {
      await invoke("stream_previous");
      return;
    }
    await invoke("prev_track");
    await syncPlaybackStatus();
  } catch (err) {
    console.error("Prev track failed:", err);
  }
});

nextBtn?.addEventListener("click", async () => {
  try {
    if (state.streamingActive) {
      await invoke("stream_next");
      return;
    }
    await invoke("next_track");
    await syncPlaybackStatus();
  } catch (err) {
    console.error("Next track failed:", err);
  }
});

// Shuffle button toggle
shuffleBtn?.addEventListener("click", async () => {
  try {
    if (state.streamingActive) {
      const next = !state.streamShuffle;
      state.streamShuffle = next; // optimistic; the player's report confirms it
      paintStreamControls();
      await invoke("stream_shuffle", { enabled: next });
      return;
    }
    const newShuffle = !state.playback.isShuffle;
    await invoke("set_shuffle", { enable: newShuffle });
    state.playback.isShuffle = newShuffle;
    shuffleBtn.classList.toggle("active", newShuffle);
    shuffleBtn.style.color = newShuffle ? "var(--accent)" : "";
  } catch (err) {
    console.error("Set shuffle failed:", err);
  }
});

// Repeat button cycle: Off -> All -> One -> Off (Apple's own order)
const repeatCycle = [
  { mode: 0, label: "Off" },
  { mode: 2, label: "All" },
  { mode: 1, label: "One" },
];
repeatBtn?.addEventListener("click", async () => {
  try {
    if (state.streamingActive) {
      const idx = repeatCycle.findIndex((m) => m.mode === state.streamRepeat);
      const next = repeatCycle[(idx + 1) % repeatCycle.length];
      state.streamRepeat = next.mode;
      paintStreamControls();
      await invoke("stream_repeat", { mode: next.label });
      return;
    }
    const idx = repeatCycle.findIndex((m) => m.label === state.playback.repeatMode);
    const next = repeatCycle[(idx + 1) % repeatCycle.length];
    await invoke("set_repeat_mode", { mode: next.label });
    state.playback.repeatMode = next.label;
    repeatBtn.classList.toggle("active", next.label !== "Off");
    repeatBtn.style.color = next.label !== "Off" ? "var(--accent)" : "";
    repeatBtn.setAttribute("title", `Repeat: ${next.label}`);
  } catch (err) {
    console.error("Set repeat failed:", err);
  }
});

// Volume control
volumeBar?.addEventListener("click", async (e) => {
  const rect = volumeBar.getBoundingClientRect();
  const x = e.clientX - rect.left;
  const pct = Math.max(0, Math.min(1, x / rect.width));
  volumeFill.style.width = `${pct * 100}%`;
  try {
    if (state.streamingActive) {
      state.streamVolume = pct;
      await invoke("stream_volume", { volume: pct });
      return;
    }
    await invoke("set_volume", { volume: pct });
    state.playback.volume = pct;
  } catch (err) {
    console.error("Set volume failed:", err);
  }
});

// Progress bar seeking
progressBar?.addEventListener("click", async (e) => {
  const rect = progressBar.getBoundingClientRect();
  const x = e.clientX - rect.left;
  const pct = Math.max(0, Math.min(1, x / rect.width));

  // Streaming owns the bar while a stream is active — the local engine is idle.
  if (state.streamingActive) {
    const total = state.streamDuration || 0;
    if (!(total > 0)) return; // duration unknown yet; ignore instead of snapping to 0
    const target = pct * total;
    state.streamPosition = target;
    state.streamAnchor = Date.now() - target * 1000;
    progressFill.style.width = `${pct * 100}%`;
    currentTime.textContent = formatTime(target);
    try {
      await invoke("stream_seek", { position: target });
    } catch (err) {
      console.error("Stream seek failed:", err);
    }
    return;
  }

  progressFill.style.width = `${pct * 100}%`;
  const targetSecs = pct * (state.playback.duration || 0);
  try {
    await invoke("seek_to", { positionSecs: targetSecs });
    state.playback.position = targetSecs;
    currentTime.textContent = formatTime(targetSecs);
  } catch (err) {
    console.error("Seek failed:", err);
  }
});

// Like button
document.getElementById("like-btn")?.addEventListener("click", (e) => {
  const btn = e.currentTarget;
  const svg = btn.querySelector("svg");
  const isLiked = svg.getAttribute("fill") === "currentColor";
  svg.setAttribute("fill", isLiked ? "none" : "currentColor");
  svg.style.color = isLiked ? "" : "var(--accent)";
});

// Mute toggle
document.getElementById("volume-btn")?.addEventListener("click", async () => {
  const isMuted = volumeFill.style.width === "0%" || volumeFill.style.width === "0px";
  const targetVol = isMuted ? 0.8 : 0.0;
  volumeFill.style.width = `${targetVol * 100}%`;
  try {
    if (state.streamingActive) {
      state.streamVolume = targetVol;
      await invoke("stream_volume", { volume: targetVol });
      return;
    }
    await invoke("set_volume", { volume: targetVol });
    state.playback.volume = targetVol;
  } catch (err) {
    console.error("Toggle mute failed:", err);
  }
});

// ─── Continuous Playback Synchronization ───────────────────────
async function syncPlaybackStatus() {
  try {
    // Streaming runs in the hidden Apple player. Its real state (playing, position,
    // duration) arrives via the `player-state` event; the local clock only smooths
    // the gaps between reports.
    if (state.streamingActive) {
      const icon = state.streamIsPlaying
        ? "M6 19h4V5H6v14zm8-14v14h4V5h-4z"
        : "M8 5v14l11-7z";
      if (playIcon && playIcon.getAttribute("d") !== icon) {
        playIcon.setAttribute("d", icon);
      }

      const pos = state.streamIsPlaying
        ? Math.min(state.streamDuration || 0, (Date.now() - state.streamAnchor) / 1000)
        : state.streamPosition || 0;

      if (state.streamDuration > 0) {
        progressFill.style.width = `${Math.min(100, (pos / state.streamDuration) * 100)}%`;
        currentTime.textContent = formatTime(pos);
        totalTime.textContent = formatTime(state.streamDuration);
      }
      return;
    }

    const status = await invoke("get_playback_status");
    if (!status) return;

    const isPlaying = status.state === "Playing";
    state.playback.isPlaying = isPlaying;
    state.playback.currentTrackId = status.current_track_id;
    state.playback.duration = status.duration_secs || 0;
    state.playback.position = status.position_secs || 0;
    state.playback.volume = status.volume;
    state.playback.isShuffle = status.is_shuffle;

    // 1. Play/Pause icon
    if (isPlaying) {
      playIcon.setAttribute("d", "M6 19h4V5H6v14zm8-14v14h4V5h-4z");
    } else {
      playIcon.setAttribute("d", "M8 5v14l11-7z");
    }

    // 2. Track Title & Artist
    if (status.current_track_name) {
      nowPlayingTitle.textContent = status.current_track_name;
      nowPlayingArtist.textContent = status.current_artist || "Unknown Artist";
    } else if (status.state === "Stopped") {
      nowPlayingTitle.textContent = "No Track Selected";
      nowPlayingArtist.textContent = "Open your library to start playing";
    }

    // 3. Timeline progress & time text
    if (state.playback.duration > 0) {
      const pct = Math.min(100, Math.max(0, (state.playback.position / state.playback.duration) * 100));
      progressFill.style.width = `${pct}%`;
      currentTime.textContent = formatTime(state.playback.position);
      totalTime.textContent = formatTime(state.playback.duration);
    } else {
      progressFill.style.width = "0%";
      currentTime.textContent = "0:00";
      totalTime.textContent = "0:00";
    }

    // 4. Highlight currently playing track in list
    document.querySelectorAll(".track-item").forEach((el) => {
      const isCurrent = el.dataset.trackId === status.current_track_id;
      el.classList.toggle("playing", isCurrent);
    });
  } catch (_err) {
    // Suppress polling error during shutdown/reload
  }
}

// 250ms smooth synchronization loop
setInterval(syncPlaybackStatus, 250);

// ─── Utility Functions ─────────────────────────────────────────
function formatTime(secs) {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function escapeHtml(str) {
  const div = document.createElement("div");
  div.textContent = str || "";
  return div.innerHTML;
}

function debounce(fn, delay) {
  let timer;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), delay);
  };
}

// ─── Init ──────────────────────────────────────────────────────
window.scanLibrary = scanLibrary;

// Initial bootstrap
checkAuthState().then(() => {
  renderListenNow();
  loadLibrary().then(() => {
    syncPlaybackStatus();
  });
});
