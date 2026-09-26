//! `cargo run -p brixo-samples --example flagfall_audio [dir]`: writes
//! Flagfall's music and sound effects as WAV files (for trailers, or to
//! use in your own game).
fn main() {
    use brixo_samples::synth;
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&dir).expect("couldn't make the folder");
    for (name, wav) in [
        ("flagfall-theme.wav", synth::flagfall_theme(false)),
        ("flagfall-theme-intense.wav", synth::flagfall_theme(true)),
        ("flag-taken.wav", synth::flag_taken()),
        ("flag-returned.wav", synth::flag_returned()),
        ("capture.wav", synth::capture_fanfare()),
        ("victory.wav", synth::victory()),
        ("horn.wav", synth::horn()),
    ] {
        std::fs::write(dir.join(name), wav).expect("couldn't write");
        println!("wrote {name}");
    }
}
