use super::Track;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

pub struct PlaylistFileHelper;

impl PlaylistFileHelper {
    pub fn export_m3u8(playlist_name: &str, tracks: &[Track], out_path: &Path) -> anyhow::Result<()> {
        let mut file = File::create(out_path)?;
        writeln!(file, "#EXTM3U")?;
        writeln!(file, "#PLAYLIST:{}", playlist_name)?;

        for track in tracks {
            let dur = track.duration_secs.round() as i64;
            writeln!(file, "#EXTINF:{},{} - {}", dur, track.artist, track.title)?;
            writeln!(file, "{}", track.file_path)?;
        }

        file.flush()?;
        Ok(())
    }

    pub fn import_m3u8(path: &Path) -> anyhow::Result<Vec<String>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let base_dir = path.parent().unwrap_or_else(|| Path::new(""));

        let mut file_paths = Vec::new();
        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Resolve relative paths if any
            let p = Path::new(trimmed);
            let resolved = if p.is_relative() {
                base_dir.join(p).to_string_lossy().to_string()
            } else {
                trimmed.to_string()
            };

            file_paths.push(resolved);
        }

        Ok(file_paths)
    }

    pub fn export_pls(tracks: &[Track], out_path: &Path) -> anyhow::Result<()> {
        let mut file = File::create(out_path)?;
        writeln!(file, "[playlist]")?;
        writeln!(file, "NumberOfEntries={}", tracks.len())?;

        for (idx, track) in tracks.iter().enumerate() {
            let i = idx + 1;
            writeln!(file, "File{}={}", i, track.file_path)?;
            writeln!(file, "Title{}={} - {}", i, track.artist, track.title)?;
            writeln!(file, "Length{}={}", i, track.duration_secs.round() as i64)?;
        }

        writeln!(file, "Version=2")?;
        file.flush()?;
        Ok(())
    }

    pub fn import_pls(path: &Path) -> anyhow::Result<Vec<String>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let base_dir = path.parent().unwrap_or_else(|| Path::new(""));

        let mut entries: std::collections::BTreeMap<usize, String> = std::collections::BTreeMap::new();
        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.starts_with("File") {
                if let Some(eq_pos) = trimmed.find('=') {
                    let key = &trimmed[4..eq_pos];
                    let val = trimmed[eq_pos + 1..].trim();
                    if let Ok(num) = key.parse::<usize>() {
                        let p = Path::new(val);
                        let resolved = if p.is_relative() {
                            base_dir.join(p).to_string_lossy().to_string()
                        } else {
                            val.to_string()
                        };
                        entries.insert(num, resolved);
                    }
                }
            }
        }

        Ok(entries.into_values().collect())
    }
}
