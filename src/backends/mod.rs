use songbird::input::Input;

pub mod dectalk;

pub trait Backend {
    async fn new() -> Self;
    async fn get_tts(&self, text: &str) -> Input;
}
