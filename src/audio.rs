use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleRate, Stream, StreamConfig};
use std::sync::{Arc, Mutex};

const SAMPLE_RATE: u32 = 16_000;
const MAX_BUFFER: usize = 10 * 60 * SAMPLE_RATE as usize; // 10 minutes

pub struct AudioBuffer {
    pub data: Vec<f32>,
    pub write_idx: usize,
    pub recording: bool,
}

impl AudioBuffer {
    fn new() -> Self {
        Self {
            data: vec![0.0; MAX_BUFFER],
            write_idx: 0,
            recording: false,
        }
    }
}

pub struct AudioCapture {
    pub buffer: Arc<Mutex<AudioBuffer>>,
    _stream: Stream,
}

#[derive(Debug, Clone)]
pub struct InputSource {
    pub name: String,
    pub description: String,
}

impl AudioCapture {
    pub fn new(device_name: &str) -> Result<Self> {
        if !device_name.is_empty() {
            set_default_source(device_name)?;
        }
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No default input device"))?;

        log::info!("Using audio device: {}", device.name().unwrap_or_default());

        let config = StreamConfig {
            channels: 1,
            sample_rate: SampleRate(SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Fixed(4000),
        };

        let buffer = Arc::new(Mutex::new(AudioBuffer::new()));
        let buf_clone = Arc::clone(&buffer);

        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut buf = buf_clone.lock().unwrap();
                if !buf.recording {
                    return;
                }
                let remaining = MAX_BUFFER.saturating_sub(buf.write_idx);
                let n = data.len().min(remaining);
                if n > 0 {
                    let start = buf.write_idx;
                    buf.data[start..start + n].copy_from_slice(&data[..n]);
                    buf.write_idx = start + n;
                }
            },
            |err| log::error!("Audio stream error: {err}"),
            None,
        )?;
        stream.play()?;

        Ok(Self {
            buffer,
            _stream: stream,
        })
    }

    pub fn start_recording(&self) {
        let mut buf = self.buffer.lock().unwrap();
        buf.write_idx = 0;
        buf.recording = true;
    }

    pub fn stop_recording(&self) -> Vec<f32> {
        let mut buf = self.buffer.lock().unwrap();
        buf.recording = false;
        let len = buf.write_idx;
        if len == 0 {
            return Vec::new();
        }
        let mut audio = buf.data[..len].to_vec();

        // Peak normalization
        let peak = audio.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        if peak > 1e-7 {
            for s in &mut audio {
                *s /= peak;
            }
        }

        audio
    }
}

/// Lists PulseAudio/PipeWire input sources and their descriptions.
///
/// `name` is the value to use in config `audio_device`.
pub fn list_input_sources() -> Result<Vec<InputSource>> {
    let output = std::process::Command::new("pactl")
        .args(["-f", "json", "list", "sources"])
        .output()
        .context("Failed to run pactl. Install pulseaudio-utils or pipewire-pulse.")?;
    if !output.status.success() {
        bail!("pactl failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let sources: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).context("Failed to parse pactl JSON output")?;
    let mut result = Vec::new();
    for source in sources {
        let name = source["name"].as_str().unwrap_or("").trim();
        if name.is_empty() || name.contains(".monitor") {
            continue;
        }

        let description = source["description"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(name);

        result.push(InputSource {
            name: name.to_string(),
            description: description.to_string(),
        });
    }

    if result.is_empty() {
        bail!("No input sources found via pactl");
    }

    Ok(result)
}

/// Set the PulseAudio default source so cpal picks it up.
pub fn set_default_source(name: &str) -> Result<()> {
    let status = std::process::Command::new("pactl")
        .args(["set-default-source", name])
        .status()
        .context("Failed to run pactl set-default-source")?;
    if !status.success() {
        bail!("pactl set-default-source failed");
    }
    Ok(())
}
