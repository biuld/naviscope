use std::path::Path;

fn main() {
    let lang_dir = Path::new("../../tree-sitter-java");
    let parser_path = lang_dir.join("src").join("parser.c");
    let scanner_path = lang_dir.join("src").join("scanner.c");

    println!("cargo:rerun-if-changed={}", parser_path.to_str().unwrap());
    if scanner_path.exists() {
        println!("cargo:rerun-if-changed={}", scanner_path.to_str().unwrap());
    }

    let mut build = cc::Build::new();
    build.file(&parser_path).include(lang_dir.join("src"));

    if scanner_path.exists() {
        build.file(&scanner_path);
    }

    build.compile("tree-sitter-java");
}
