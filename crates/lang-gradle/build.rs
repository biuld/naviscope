use std::path::Path;

fn main() {
    let lang_dir = Path::new("../../tree-sitter-groovy");
    let parser_path = lang_dir.join("src").join("parser.c");

    println!("cargo:rerun-if-changed={}", parser_path.to_str().unwrap());

    let mut build = cc::Build::new();
    build.file(&parser_path).include(lang_dir.join("src"));

    build.compile("tree-sitter-groovy");
}
