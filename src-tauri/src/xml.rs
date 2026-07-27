use std::{collections::HashMap, path::PathBuf};

use regex::Regex;

use crate::{model::DiagnosticDto, vfs::Vfs};

pub struct ExpandedXml {
    pub text: String,
    pub source: PathBuf,
    pub diagnostics: Vec<DiagnosticDto>,
}

pub fn load_expanded(
    vfs: &Vfs,
    virtual_path: &str,
    entity_base: &str,
) -> Result<ExpandedXml, String> {
    let (text, source) = vfs.read_text(virtual_path)?;
    let mut diagnostics = Vec::new();
    let text = expand_entities(vfs, &text, entity_base, 0, &mut diagnostics);
    Ok(ExpandedXml {
        text,
        source,
        diagnostics,
    })
}

fn expand_entities(
    vfs: &Vfs,
    input: &str,
    entity_base: &str,
    depth: usize,
    diagnostics: &mut Vec<DiagnosticDto>,
) -> String {
    if depth > 8 {
        diagnostics.push(DiagnosticDto::error(
            "xml-entity-depth",
            "External XML entity expansion exceeded eight levels",
            None,
        ));
        return strip_doctype(input);
    }

    let entity_re =
        Regex::new(r#"(?is)<!ENTITY\s+([A-Za-z_][A-Za-z0-9_.-]*)\s+SYSTEM\s+["']([^"']+)["']\s*>"#)
            .expect("valid entity regex");
    let mut entities = HashMap::new();
    for captures in entity_re.captures_iter(input) {
        entities.insert(captures[1].to_string(), captures[2].to_string());
    }

    let mut output = strip_doctype(input);
    for (name, system_path) in entities {
        let virtual_path = join_virtual(entity_base, &system_path);
        let replacement = match vfs.read_text(&virtual_path) {
            Ok((text, _)) => {
                let nested = expand_entities(vfs, &text, entity_base, depth + 1, diagnostics);
                strip_xml_declaration(&nested).to_string()
            }
            Err(_) => {
                diagnostics.push(DiagnosticDto::error(
                    "missing-xml-entity",
                    format!("External entity {name} could not be resolved: {virtual_path}"),
                    Some(virtual_path.clone()),
                ));
                String::new()
            }
        };
        output = output.replace(&format!("&{name};"), &replacement);
    }
    strip_xml_declaration(&output).to_string()
}

fn join_virtual(base: &str, path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.to_lowercase().starts_with("tabledata/") {
        normalized
    } else {
        format!("{}/{}", base.trim_end_matches(['/', '\\']), normalized)
    }
}

fn strip_xml_declaration(input: &str) -> &str {
    let trimmed = input.trim_start_matches('\u{feff}').trim_start();
    if !trimmed.starts_with("<?xml") {
        return trimmed;
    }
    trimmed
        .find("?>")
        .map(|end| &trimmed[end + 2..])
        .unwrap_or(trimmed)
}

fn strip_doctype(input: &str) -> String {
    let Some(start) = input.find("<!DOCTYPE") else {
        return input.to_string();
    };
    let bytes = input.as_bytes();
    let mut quote = None;
    let mut bracket_depth = 0usize;
    let mut index = start + "<!DOCTYPE".len();
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'>' if bracket_depth == 0 => {
                    return format!("{}{}", &input[..start], &input[index + 1..]);
                }
                _ => {}
            }
        }
        index += 1;
    }
    input.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_internal_doctype_subset() {
        let xml = r#"<?xml version="1.0"?><!DOCTYPE A [<!ENTITY X SYSTEM "x.xml">]><A>&X;</A>"#;
        let stripped = strip_doctype(xml);
        assert!(!stripped.contains("DOCTYPE"));
        assert!(stripped.contains("<A>&X;</A>"));
    }
}
