(function () {
  // Windows injects this script into EVERY frame (wry's `for_main_frame_only` is
  // ignored there). music.apple.com's subframes run their own MusicKit instance
  // with no queue, which shares the persisted volume but reports an empty
  // now-playing item — so an unguarded subframe publishes an empty `mk_state`
  // that fights the real one and makes the now-playing bar flicker. Only the top
  // document may drive playback.
  if (window.top !== window.self) return;

  window.__mkReady = new Promise(function (res) { window.__mkResolve = res; });
  window.__mkQueue = window.__mkQueue || [];

  function got() {
    try {
      if (window.MusicKit && typeof window.MusicKit.getInstance === 'function') {
        var m = window.MusicKit.getInstance();
        if (m) { window.__mkResolve(m); return true; }
      }
    } catch (e) {}
    return false;
  }

  // State leaves the page through a cookie. music.apple.com's CSP blocks fetch()
  // to 127.0.0.1, and WebView2 does not mirror document.title to the native
  // window title, but `document.cookie` is first-party and CSP-immune. Rust polls
  // the webview cookie jar (the same API the sign-in flow uses) and re-emits the
  // value as `player-state`.
  function publish() {
    try {
      var m = window.MusicKit.getInstance();
      var item = m.nowPlayingItem || null;
      var attrs = (item && item.attributes) || null;
      var json = JSON.stringify({
        isPlaying: !!m.isPlaying,
        position: m.currentPlaybackTime || 0,
        duration: m.currentPlaybackDuration || 0,
        id: item && item.id ? item.id : null,
        title: attrs ? attrs.name : null,
        artist: attrs ? attrs.artistName : null,
        artwork: attrs && attrs.artwork ? attrs.artwork.url : null,
        album: attrs ? attrs.albumName : null,
        shuffle: typeof m.shuffleMode === 'number' ? m.shuffleMode : 0,
        repeat: typeof m.repeatMode === 'number' ? m.repeatMode : 0,
        volume: typeof m.volume === 'number' ? m.volume : null,
        queueLength: m.queue && typeof m.queue.length === 'number' ? m.queue.length : 0
      });
      document.cookie = 'mk_state=' + encodeURIComponent(json) + '; path=/; max-age=300; Secure; SameSite=Lax';
      publishQueue(m);
    } catch (e) {}
  }

  // The Up Next list rides in its own cookie, base64-encoded so quotes/braces do
  // not blow past the per-cookie size limit. Only the current item and the next 40
  // are sent, and only when they actually change.
  var __lastQueueSig = '';
  function publishQueue(m) {
    try {
      var q = m.queue;
      var items = (q && q.items) ? q.items : q;
      if (!items || typeof items.length !== 'number') return;
      var idx = 0;
      if (q && typeof q.position === 'number') {
        idx = q.position;
      } else {
        var cur = m.nowPlayingItem;
        if (cur) {
          for (var k = 0; k < items.length; k++) {
            if (items[k] && items[k].id === cur.id) { idx = k; break; }
          }
        }
      }
      var slice = [];
      for (var j = idx; j < items.length && slice.length < 40; j++) {
        var it = items[j];
        var a = (it && it.attributes) || {};
        slice.push({
          i: it && it.id ? it.id : null,
          n: a.name || '',
          a: a.artistName || '',
          d: a.durationInMillis || 0
        });
      }
      var sig = idx + '|' + slice.map(function (x) { return x.i; }).join(',');
      if (sig === __lastQueueSig) return;
      __lastQueueSig = sig;
      var payload = btoa(unescape(encodeURIComponent(JSON.stringify({ base: idx, items: slice }))));
      if (payload) document.cookie = 'mk_queue=' + payload + '; path=/; max-age=300; Secure; SameSite=Lax';
    } catch (e) {}
  }

  window.__mkDo = async function (cmd, arg, nonce) {
    // Each user action carries a nonce; a re-eval carries the same one,
    // so only the first attempt runs. A fresh click gets a fresh nonce.
    if (nonce) {
      if (window.__mkNonce === nonce) return;
      window.__mkNonce = nonce;
    }
    var m = await window.__mkReady;
    if (cmd === 'play') {
      // arg is JSON: { startWith, songs[] }. MusicKit owns the queue so that
      // next/previous/shuffle/repeat behave like Apple Music instead of replaying
      // one song forever.
      var spec = null;
      try { spec = JSON.parse(String(arg)); } catch (e) { spec = null; }
      var songs = spec && Array.isArray(spec.songs) ? spec.songs.filter(Boolean) : [];
      var startWith = (spec && spec.startWith) || songs[0] || String(arg);
      // Apple's album "Shuffle" is the same queue with shuffle already on; a plain
      // album Play turns it off. A bare track click leaves the mode untouched.
      if (spec && spec.shuffle === true) {
        try { m.shuffleMode = 1; } catch (e) {}
      } else if (spec && spec.shuffle === false) {
        try { m.shuffleMode = 0; } catch (e) {}
      }
      if (songs.length > 1) {
        try {
          await m.setQueue({ songs: songs, startWith: startWith });
        } catch (e) {
          await m.setQueue({ songs: songs });
          var idx = songs.indexOf(startWith);
          if (idx > 0) await m.changeToMediaAtIndex(idx);
        }
      } else {
        await m.setQueue({ song: String(startWith) });
      }
      return m.play();
    }
    if (cmd === 'pause') return m.pause();
    if (cmd === 'resume') return m.play();
    if (cmd === 'stop') return m.stop();
    if (cmd === 'seek') return m.seekToTime(Number(arg));
    if (cmd === 'next') return m.skipToNextItem();
    if (cmd === 'queuejump') return m.changeToMediaAtIndex(Number(arg));
    if (cmd === 'previous') {
      // Apple's own rule: restart the track if we are more than 3s in.
      if (typeof m.currentPlaybackTime === 'number' && m.currentPlaybackTime > 3) {
        return m.seekToTime(0);
      }
      return m.skipToPreviousItem();
    }
    if (cmd === 'shuffle') { m.shuffleMode = Number(arg) ? 1 : 0; return; }
    if (cmd === 'repeat') { m.repeatMode = Number(arg) || 0; return; }
    if (cmd === 'volume') {
      m.volume = Math.max(0, Math.min(1, Number(arg)));
      return;
    }
  };

  function run(job) {
    window.__mkDo(job.cmd, job.arg, job.nonce)
      .then(publish)
      .catch(function (e) { console.error('mk', e); });
  }

  // Runs every command queued so far, including ones Rust pushed before this
  // script existed in the current document.
  window.__mkDrain = function () {
    var jobs = window.__mkQueue.splice(0);
    jobs.forEach(run);
  };

  window.__mkReady.then(function (m) {
    // Publish on real player transitions instead of waiting for the next tick.
    ['playbackStateDidChange', 'nowPlayingItemDidChange', 'playbackTimeDidChange'].forEach(function (name) {
      try { m.addEventListener(name, publish); } catch (e) {}
    });
    publish();
    setInterval(publish, 500);
  });

  if (!got()) {
    var tries = 0;
    var t = setInterval(function () {
      tries += 1;
      if (got() || tries > 200) clearInterval(t);
    }, 300);
  }

  window.__mkDrain();
})();
