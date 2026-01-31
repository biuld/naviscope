use std::path::Path;

fn main() {
    let grammars = vec![
        ("java", "tree-sitter-java"),
        ("groovy", "tree-sitter-groovy"),
    ];

    for (language, dir) in grammars {
        let lang_dir = Path::new("../../").join(dir);
        let parser_path = lang_dir.join("src").join("parser.c");
        let scanner_path = lang_dir.join("src").join("scanner.c");

        println!("cargo:rerun-if-changed={}", parser_path.to_str().unwrap());
        println!("cargo:rerun-if-changed={}", scanner_path.to_str().unwrap());

        let mut build = cc::Build::new();
        build.file(&parser_path).include(lang_dir.join("src"));

        if scanner_path.exists() {
            build.file(&scanner_path);
        }

        build.compile(format!("tree-sitter-{}", language).as_str());
    }
}
