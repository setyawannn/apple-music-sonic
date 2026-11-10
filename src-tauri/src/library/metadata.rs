use std::fs::File;
use std::path::Path;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::{MetadataOptions, StandardTagKey, Value};
use symphonia::core::probe::Hint;

#[derive(Debug, Default, Clone)]
pub struct ExtractedMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track_number: u32,
    pub disc_number: u32,
    pub duration_secs: f64,
    pub genre: String,
    pub year: i32,
    pub artwork_data: Option<Vec<u8>>,
    pub artwork_mime: Option<String>,
}

pub fn extract_metadata<P: AsRef<Path>>(path: P) -> anyhow::Result<ExtractedMetadata> {
    let path = path.as_ref();
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let meta_opts = MetadataOptions::default();

    let mut probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &meta_opts)?;

    let mut meta = ExtractedMetadata {
        title: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown Track")
            .to_string(),
        artist: "Unknown Artist".to_string(),
        album: "Unknown Album".to_string(),
        album_artist: String::new(),
        track_number: 0,
        disc_number: 1,
        duration_secs: 0.0,
        genre: String::new(),
        year: 0,
        artwork_data: None,
        artwork_mime: None,
    };

    // Calculate duration from first audio track
    if let Some(track) = probed.format.tracks().first() {
        let params = &track.codec_params;
        if let (Some(n_frames), Some(time_base)) = (params.n_frames, params.time_base) {
            let time = time_base.calc_time(n_frames);
            meta.duration_secs = time.seconds as f64 + time.frac;
        }
    }

    // Process metadata revisions
    let mut parse_tag = |tag: &symphonia::core::meta::Tag| {
        if let Some(std_key) = tag.std_key {
            match std_key {
                StandardTagKey::TrackTitle => meta.title = value_to_string(&tag.value),
                StandardTagKey::Artist => meta.artist = value_to_string(&tag.value),
                StandardTagKey::Album => meta.album = value_to_string(&tag.value),
                StandardTagKey::AlbumArtist => meta.album_artist = value_to_string(&tag.value),
                StandardTagKey::TrackNumber => meta.track_number = parse_int_or_split(&tag.value),
                StandardTagKey::DiscNumber => meta.disc_number = parse_int_or_split(&tag.value).max(1),
                StandardTagKey::Genre => meta.genre = value_to_string(&tag.value),
                StandardTagKey::ReleaseDate => {
                    meta.year = parse_year(&value_to_string(&tag.value));
                }
                _ => {}
            }
        }
    };

    // 1. Check format reader metadata
    if let Some(rev) = probed.format.metadata().current() {
        for tag in rev.tags() {
            parse_tag(tag);
        }
        if let Some(vis) = rev.visuals().first() {
            meta.artwork_data = Some(vis.data.to_vec());
            meta.artwork_mime = Some(vis.media_type.clone());
        }
    }

    // 2. Check probed container metadata if not yet found
    if let Some(probed_meta) = probed.metadata.get() {
        if let Some(rev) = probed_meta.current() {
            for tag in rev.tags() {
                parse_tag(tag);
            }
            if meta.artwork_data.is_none() {
                if let Some(vis) = rev.visuals().first() {
                    meta.artwork_data = Some(vis.data.to_vec());
                    meta.artwork_mime = Some(vis.media_type.clone());
                }
            }
        }
    }

    Ok(meta)
}

fn value_to_string(val: &Value) -> String {
    match val {
        Value::String(s) => s.trim().to_string(),
        Value::UnsignedInt(u) => u.to_string(),
        Value::SignedInt(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Boolean(b) => b.to_string(),
        _ => String::new(),
    }
}

fn parse_int_or_split(val: &Value) -> u32 {
    match val {
        Value::UnsignedInt(u) => *u as u32,
        Value::SignedInt(i) => (*i).max(0) as u32,
        Value::String(s) => {
            let part = s.split('/').next().unwrap_or("0");
            part.trim().parse::<u32>().unwrap_or(0)
        }
        _ => 0,
    }
}

fn parse_year(val: &str) -> i32 {
    let digits: String = val.chars().filter(|c| c.is_ascii_digit()).take(4).collect();
    digits.parse::<i32>().unwrap_or(0)
}
