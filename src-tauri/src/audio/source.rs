use std::fs::File;
use std::path::Path;
use std::time::Duration;
use symphonia::core::audio::{SampleBuffer, SignalSpec};
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::{Error, Result as SymphoniaResult};
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

pub struct SymphoniaSource {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    sample_rate: u32,
    channels: u16,
    duration_secs: Option<f64>,
    sample_buf: Option<SampleBuffer<f32>>,
    current_spec: Option<SignalSpec>,
    sample_index: usize,
}

// Safety: FormatReader and Decoder in Symphonia default backends are Send.
unsafe impl Send for SymphoniaSource {}

impl SymphoniaSource {
    pub fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
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

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &meta_opts)?;

        let reader = probed.format;

        // Find the first supported audio track with a non-null codec
        let track = reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| anyhow::anyhow!("No supported audio track found in file"))?;

        let track_id = track.id;
        let codec_params = track.codec_params.clone();

        let sample_rate = codec_params
            .sample_rate
            .ok_or_else(|| anyhow::anyhow!("Unknown sample rate"))?;
        let channels = codec_params
            .channels
            .map(|c| c.count() as u16)
            .unwrap_or(2);

        let duration_secs = if let (Some(n_frames), Some(time_base)) = (codec_params.n_frames, codec_params.time_base) {
            let time = time_base.calc_time(n_frames);
            Some(time.seconds as f64 + time.frac)
        } else {
            None
        };

        let dec_opts = DecoderOptions::default();
        let decoder = symphonia::default::get_codecs().make(&codec_params, &dec_opts)?;

        Ok(Self {
            reader,
            decoder,
            track_id,
            sample_rate,
            channels,
            duration_secs,
            sample_buf: None,
            current_spec: None,
            sample_index: 0,
        })
    }

    pub fn duration_secs(&self) -> Option<f64> {
        self.duration_secs
    }

    pub fn seek(&mut self, target_secs: f64) -> SymphoniaResult<()> {
        let seek_to = SeekTo::Time {
            time: Time::from(target_secs.max(0.0)),
            track_id: Some(self.track_id),
        };

        self.reader.seek(SeekMode::Coarse, seek_to)?;
        self.decoder.reset();
        self.sample_buf = None;
        self.current_spec = None;
        self.sample_index = 0;
        Ok(())
    }

    fn read_next_packet(&mut self) -> Option<()> {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(pkt) => pkt,
                Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return None;
                }
                Err(Error::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(_) => return None,
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let capacity = decoded.capacity() as u64;

                    if self.current_spec.as_ref() != Some(&spec) || self.sample_buf.is_none() {
                        self.sample_buf = Some(SampleBuffer::<f32>::new(capacity, spec));
                        self.current_spec = Some(spec);
                    }

                    if let Some(buf) = self.sample_buf.as_mut() {
                        buf.copy_interleaved_ref(decoded);
                        self.sample_index = 0;
                        return Some(());
                    }
                }
                Err(Error::DecodeError(msg)) => {
                    log::warn!("Decode warning (skipped): {}", msg);
                    continue;
                }
                Err(_) => return None,
            }
        }
    }
}

impl Iterator for SymphoniaSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ref buf) = self.sample_buf {
                let samples = buf.samples();
                if self.sample_index < samples.len() {
                    let sample = samples[self.sample_index];
                    self.sample_index += 1;
                    return Some(sample);
                }
            }

            // Current buffer exhausted, read next packet
            if self.read_next_packet().is_none() {
                return None;
            }
        }
    }
}

impl rodio::Source for SymphoniaSource {
    fn current_frame_len(&self) -> Option<usize> {
        self.sample_buf
            .as_ref()
            .map(|buf| buf.samples().len().saturating_sub(self.sample_index))
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        self.duration_secs.map(Duration::from_secs_f64)
    }
}
