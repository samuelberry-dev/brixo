//! `brixo-samples [tycoon|spire|flag|gears] [file]`: writes a sample game as a file,
//! to open and change in Brixo Studio. (The Brixo website publishes both to
//! its catalog by itself.)
fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "tycoon".to_string());
    let (model, default) = match which.as_str() {
        "battle" | "spire" => (brixo_samples::spire_wars(), "spire-wars.brixo"),
        "flag" | "flagfall" | "ctf" => (brixo_samples::flagfall(), "flagfall.brixo"),
        "gear" | "gears" | "range" => (brixo_samples::gears::gear_range(), "gear-range.brixo"),
        _ => (brixo_samples::coin_tycoon(), "coin-tycoon.brixo"),
    };
    let path = std::env::args().nth(2).unwrap_or_else(|| default.to_string());
    match model.save_file(&path) {
        Ok(()) => println!("Wrote {path}"),
        Err(e) => eprintln!("Couldn't write {path}: {e}"),
    }
}
