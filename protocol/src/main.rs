fn main() {
    if let Err(err) = liminaldb_protocol_codegen::generate() {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}
