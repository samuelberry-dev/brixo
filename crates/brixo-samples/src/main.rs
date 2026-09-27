//! `brixo-samples [tycoon|spire|flag|gears|lab|speedway] [file]`: writes a sample game as a file,
//! to open and change in Brixo Studio. (The Brixo website publishes both to
//! its catalog by itself.)
fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "tycoon".to_string());
    let (model, default) = match which.as_str() {
        "battle" | "spire" => (brixo_samples::spire_wars(), "spire-wars.brixo"),
        "flag" | "flagfall" | "ctf" => (brixo_samples::flagfall(), "flagfall.brixo"),
        "gear" | "gears" | "range" => (brixo_samples::gears::gear_range(), "gear-range.brixo"),
        "lab" | "test" => (brixo_samples::lab::test_lab(), "test-lab.brixo"),
        "kart" | "karts" | "speedway" => (brixo_samples::speedway::brickport_speedway(), "brickport-speedway.brixo"),
        _ => (brixo_samples::coin_tycoon(), "coin-tycoon.brixo"),
    };
    let path = std::env::args().nth(2).unwrap_or_else(|| default.to_string());
    match model.save_file(&path) {
        Ok(()) => println!("Wrote {path}"),
        Err(e) => eprintln!("Couldn't write {path}: {e}"),
    }
}
