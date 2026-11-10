use crate::library::Track;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueueManager {
    tracks: Vec<Track>,
    current_index: Option<usize>,
    repeat_mode: RepeatMode,
    shuffle: bool,
    shuffled_indices: Vec<usize>,
    shuffle_pos: usize,
}

impl QueueManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_queue(&mut self, tracks: Vec<Track>, start_index: Option<usize>) -> Option<Track> {
        self.tracks = tracks;
        let idx = start_index.unwrap_or(0);
        if idx < self.tracks.len() {
            self.current_index = Some(idx);
            self.rebuild_shuffle();
            self.tracks.get(idx).cloned()
        } else {
            self.current_index = None;
            self.shuffled_indices.clear();
            self.shuffle_pos = 0;
            None
        }
    }

    pub fn play_track(&mut self, track: Track) -> Track {
        if let Some(pos) = self.tracks.iter().position(|t| t.id == track.id) {
            self.current_index = Some(pos);
            self.rebuild_shuffle();
            self.tracks[pos].clone()
        } else {
            self.tracks.push(track.clone());
            let new_pos = self.tracks.len() - 1;
            self.current_index = Some(new_pos);
            self.rebuild_shuffle();
            track
        }
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current_index.and_then(|idx| self.tracks.get(idx))
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.repeat_mode
    }

    pub fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.repeat_mode = mode;
    }

    pub fn is_shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn set_shuffle(&mut self, enable: bool) {
        self.shuffle = enable;
        if enable {
            self.rebuild_shuffle();
        }
    }

    pub fn next(&mut self) -> Option<Track> {
        if self.tracks.is_empty() {
            return None;
        }

        if self.repeat_mode == RepeatMode::One {
            return self.current_track().cloned();
        }

        if self.shuffle {
            if self.shuffled_indices.is_empty() {
                self.rebuild_shuffle();
            }

            if self.shuffle_pos + 1 < self.shuffled_indices.len() {
                self.shuffle_pos += 1;
                let next_idx = self.shuffled_indices[self.shuffle_pos];
                self.current_index = Some(next_idx);
                return self.tracks.get(next_idx).cloned();
            } else if self.repeat_mode == RepeatMode::All {
                self.rebuild_shuffle();
                self.shuffle_pos = 0;
                let next_idx = self.shuffled_indices[0];
                self.current_index = Some(next_idx);
                return self.tracks.get(next_idx).cloned();
            } else {
                return None;
            }
        }

        // Sequential mode
        let cur = self.current_index.unwrap_or(0);
        if cur + 1 < self.tracks.len() {
            let next_idx = cur + 1;
            self.current_index = Some(next_idx);
            self.tracks.get(next_idx).cloned()
        } else if self.repeat_mode == RepeatMode::All {
            self.current_index = Some(0);
            self.tracks.first().cloned()
        } else {
            None
        }
    }

    pub fn prev(&mut self) -> Option<Track> {
        if self.tracks.is_empty() {
            return None;
        }

        if self.repeat_mode == RepeatMode::One {
            return self.current_track().cloned();
        }

        if self.shuffle {
            if self.shuffle_pos > 0 {
                self.shuffle_pos -= 1;
                let prev_idx = self.shuffled_indices[self.shuffle_pos];
                self.current_index = Some(prev_idx);
                return self.tracks.get(prev_idx).cloned();
            } else if self.repeat_mode == RepeatMode::All && !self.shuffled_indices.is_empty() {
                self.shuffle_pos = self.shuffled_indices.len() - 1;
                let prev_idx = self.shuffled_indices[self.shuffle_pos];
                self.current_index = Some(prev_idx);
                return self.tracks.get(prev_idx).cloned();
            } else {
                let cur = self.shuffled_indices.first().copied().unwrap_or(0);
                self.current_index = Some(cur);
                return self.tracks.get(cur).cloned();
            }
        }

        // Sequential mode
        let cur = self.current_index.unwrap_or(0);
        if cur > 0 {
            let prev_idx = cur - 1;
            self.current_index = Some(prev_idx);
            self.tracks.get(prev_idx).cloned()
        } else if self.repeat_mode == RepeatMode::All {
            let last_idx = self.tracks.len() - 1;
            self.current_index = Some(last_idx);
            self.tracks.get(last_idx).cloned()
        } else {
            self.current_index = Some(0);
            self.tracks.first().cloned()
        }
    }

    fn rebuild_shuffle(&mut self) {
        let n = self.tracks.len();
        if n == 0 {
            self.shuffled_indices.clear();
            self.shuffle_pos = 0;
            return;
        }

        let mut indices: Vec<usize> = (0..n).collect();
        let mut seed = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(123456789)) as u64;

        for i in (1..n).rev() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let j = (seed % ((i + 1) as u64)) as usize;
            indices.swap(i, j);
        }

        // Move the currently active track to index 0 so it stays first
        if let Some(cur) = self.current_index {
            if let Some(pos) = indices.iter().position(|&x| x == cur) {
                indices.swap(0, pos);
            }
        }

        self.shuffled_indices = indices;
        self.shuffle_pos = 0;
    }
}
