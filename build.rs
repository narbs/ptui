// build.rs: Embed all locale files into a generated Rust source file
use std::fs;
use std::path::Path;
use std::io::Write;

fn main() {
    let locales_dir = "./locales";
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("locales.rs");
    let mut out = fs::File::create(&dest_path).unwrap();

    writeln!(out, "use std::collections::HashMap;").unwrap();
    writeln!(out, "pub fn get_embedded_locales() -> HashMap<&'static str, &'static str> {{").unwrap();
    writeln!(out, "    let mut map = HashMap::new();").unwrap();

    for entry in fs::read_dir(locales_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            let locale = path.file_name().unwrap().to_string_lossy();
            let ftl_path = path.join("main.ftl");
            if ftl_path.exists() {
                let content = fs::read_to_string(&ftl_path).unwrap();
                // Escape double quotes and backslashes
                let content_escaped = content.replace("\\", "\\\\").replace("\"", "\\\"");
                writeln!(out, "    map.insert(\"{}\", \"{}\");", locale, content_escaped).unwrap();
            }
        }
    }
    writeln!(out, "    map").unwrap();
    writeln!(out, "}}").unwrap();
}
