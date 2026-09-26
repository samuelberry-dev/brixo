//! `brixo-sounds <folder>`: writes every built-in sound and music loop as a
//! WAV file, to listen to or use elsewhere.
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "brixo-sounds".into()));
    std::fs::create_dir_all(&dir).expect("couldn't create the folder");
    for name in brixo_core::SOUNDS {
        brixo_audio::write_wav(&dir.join(format!("{name}.wav")), &brixo_audio::sound(name).unwrap()).unwrap();
    }
    for name in ["sunny", "rush"] {
        brixo_audio::write_wav(&dir.join(format!("music-{name}.wav")), &brixo_audio::music(name).unwrap()).unwrap();
    }
    println!("Wrote Brixo's sounds to {}", dir.display());
}
