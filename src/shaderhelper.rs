use std::collections::HashSet;

use regex::Regex;

pub fn include_files<S: AsRef<str>, F: Fn(&dyn AsRef<str>) -> Option<String>>(
    shader: S,
    lookup: F,
) -> String {
    include_files_recusive(shader, &mut HashSet::new(), &lookup)
}

fn include_files_recusive<S: AsRef<str>, F: Fn(&dyn AsRef<str>) -> Option<String>>(
    shader: S,
    seen: &mut HashSet<String>,
    lookup: &F,
) -> String {
    let mut output = String::new();
    let shader_str = shader.as_ref();
    let mut last = 0;
    let include_re = Regex::new(r#"(?m)^#include\s+"([^"]+)""#).unwrap();
    while last < shader_str.len() {
        if let Some(captures) = include_re.captures(&shader_str[last..]) {
            let start = last + captures.get(0).unwrap().start();
            let end = last + captures.get(0).unwrap().end();
            output.push_str(&shader_str[last..start]);
            let include_name = captures.get(1).unwrap().as_str();
            if !seen.contains(include_name) {
                seen.insert(include_name.to_string());
                output.push_str("\n");
                if let Some(include_source) = lookup(&include_name) {
                    output.push_str(&include_files_recusive(include_source, seen, lookup));
                } else {
                    eprintln!("Included file not found: {}", include_name);
                }
            }
            last = end;
        } else {
            output.push_str(&shader_str[last..]);
            break;
        }
    }

    return output;
}
