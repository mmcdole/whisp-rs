use anyhow::Result;
use std::collections::VecDeque;
use std::sync::mpsc;
use std::thread;

use sherpa_rs::transducer::{TransducerConfig, TransducerRecognizer};

const MAX_QUEUE: usize = 20;

struct Transcriber {
    recognizer: TransducerRecognizer,
}

impl Transcriber {
    fn new(paths: &crate::config::ModelPaths) -> Result<Self> {
        let config = TransducerConfig {
            encoder: paths.encoder.to_string_lossy().into_owned(),
            decoder: paths.decoder.to_string_lossy().into_owned(),
            joiner: paths.joiner.to_string_lossy().into_owned(),
            tokens: paths.tokens.to_string_lossy().into_owned(),
            sample_rate: 16000,
            feature_dim: 80,
            num_threads: 4,
            decoding_method: "greedy_search".into(),
            model_type: "nemo_transducer".into(),
            ..Default::default()
        };
        log::info!("Loading sherpa transducer model");
        let recognizer = TransducerRecognizer::new(config)
            .map_err(|e| anyhow::anyhow!("Failed to create sherpa recognizer: {e}"))?;
        Ok(Self { recognizer })
    }

    fn transcribe(&mut self, audio: &[f32]) -> Result<String> {
        let text = self.recognizer.transcribe(16000, audio);
        Ok(text.trim().to_string())
    }
}

pub fn spawn_worker(
    paths: crate::config::ModelPaths,
    audio_rx: mpsc::Receiver<Vec<f32>>,
    text_tx: mpsc::Sender<String>,
) {
    thread::spawn(move || {
        let mut transcriber =
            Transcriber::new(&paths).expect("failed to init sherpa backend");

        log::info!("Transcription worker ready");

        let mut queue: VecDeque<Vec<f32>> = VecDeque::with_capacity(MAX_QUEUE);
        loop {
            let audio = match audio_rx.recv() {
                Ok(a) => a,
                Err(_) => break,
            };
            queue.push_back(audio);

            while let Ok(a) = audio_rx.try_recv() {
                queue.push_back(a);
                if queue.len() > MAX_QUEUE {
                    queue.pop_front();
                }
            }

            while let Some(audio) = queue.pop_front() {
                match transcriber.transcribe(&audio) {
                    Ok(text) if !text.is_empty() => {
                        let _ = text_tx.send(text);
                    }
                    Ok(_) => log::debug!("Empty transcription result"),
                    Err(e) => log::error!("Transcription error: {e}"),
                }
            }
        }
    });
}
