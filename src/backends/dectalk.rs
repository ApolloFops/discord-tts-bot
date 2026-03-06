use std::{io::Cursor, sync::Arc};

use dectalk::TTSHandle;
use hound::{SampleFormat, WavSpec, WavWriter};
use songbird::input::{codecs::RawReader, AudioStream, Input, LiveInput};
use symphonia::core::{
    formats::FormatReader,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    probe::Hint,
};
use tokio::sync::Mutex;

use crate::backends::Backend;

const DATA_BUFFER_SIZE: usize = 4096;
const INDEX_BUFFER_SIZE: usize = 128;

const WAV_SPEC: WavSpec = WavSpec {
    channels: 1,
    sample_rate: 11025,
    bits_per_sample: 16,
    sample_format: SampleFormat::Int,
};

pub struct DECTalkBackend {
    handle: Arc<Mutex<TTSHandle>>,
}

impl Backend for DECTalkBackend {
    async fn new() -> Self {
        let backend = DECTalkBackend {
            handle: Arc::new(Mutex::new(TTSHandle::new())),
        };

        let mut dectalk_lock = backend.handle.lock().await;
        dectalk_lock
            .startup(0, 0)
            .expect("Failed to start up DECTalk");
        dectalk_lock
            .open_in_memory(dectalk::DtTTSFormat::WaveFormat1M16)
            .expect("Failed to open DECTalk in memory");
        dectalk_lock
            .create_buffer(DATA_BUFFER_SIZE, INDEX_BUFFER_SIZE)
            .expect("Failed to create DECTalk speech-to-memory buffer");
        drop(dectalk_lock);

        backend
    }

    async fn get_tts(&self, text: &str) -> Input {
        let mut handle = self.handle.lock().await;

        // Get the raw data
        let raw_data = handle
            .speak(text, dectalk::DtTTSFlags::Force)
            .expect("Failed to queue speech")
            .await;

        // Set up the WAV header
        // This is needed otherwise Songbird gets angy
        let mut cursor = Cursor::new(Vec::new());
        let mut writer = WavWriter::new(&mut cursor, WAV_SPEC).unwrap();

        for chunk in raw_data.chunks_exact(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            writer.write_sample(sample).unwrap();
        }

        writer.finalize().unwrap();

        // Set the cursor position to the start of the file
        cursor.set_position(0);

        // Set up the Input
        let input = Box::new(cursor);
        let mut hint = Hint::new();
        hint.with_extension("wav");

        let raw_reader = LiveInput::Raw(AudioStream {
            input,
            hint: Some(hint),
        });

        // Return the resulting Input
        Input::Live(raw_reader, None)
    }
}
