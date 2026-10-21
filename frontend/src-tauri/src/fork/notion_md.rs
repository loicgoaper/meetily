// fork/notion_md.rs
//
// Markdown (as produced by the summary model / BlockNote) to Notion blocks: headings,
// paragraphs, bulleted/numbered lists, quotes, dividers and tables, with inline bold,
// italic, code and links. Notion limits rich text to 2000 characters per item.

use serde_json::{json, Value};

const MAX_TEXT: usize = 1900;

/// Inline Markdown to Notion rich text items.
pub fn rich_text(input: &str) -> Vec<Value> {
    let mut items: Vec<Value> = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut buf = String::new();
    let (mut bold, mut italic) = (false, false);

    let flush = |buf: &mut String, items: &mut Vec<Value>, bold: bool, italic: bool, code: bool, link: Option<&str>| {
        if buf.is_empty() {
            return;
        }
        for piece in split_len(buf, MAX_TEXT) {
            let mut text = json!({ "content": piece });
            if let Some(url) = link {
                text["link"] = json!({ "url": url });
            }
            items.push(json!({
                "type": "text",
                "text": text,
                "annotations": { "bold": bold, "italic": italic, "code": code }
            }));
        }
        buf.clear();
    };

    while i < chars.len() {
        let c = chars[i];
        // **bold**
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            flush(&mut buf, &mut items, bold, italic, false, None);
            bold = !bold;
            i += 2;
            continue;
        }
        // *italic* or _italic_ (only when it looks like emphasis, not "a_b" identifiers)
        if c == '*' || c == '_' {
            let prev_is_word = i > 0 && chars[i - 1].is_alphanumeric();
            let next = chars.get(i + 1).copied();
            let closes = italic && i > 0 && chars[i - 1] != ' ';
            let opens = !italic
                && next.map(|n| n != ' ' && n != c).unwrap_or(false)
                && (c == '*' || !prev_is_word)
                && chars[i + 1..].contains(&c);
            if closes || opens {
                flush(&mut buf, &mut items, bold, italic, false, None);
                italic = !italic;
                i += 1;
                continue;
            }
        }
        // `code`
        if c == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|&x| x == '`') {
                flush(&mut buf, &mut items, bold, italic, false, None);
                let code: String = chars[i + 1..i + 1 + end].iter().collect();
                let mut code_buf = code;
                flush(&mut code_buf, &mut items, bold, italic, true, None);
                i += end + 2;
                continue;
            }
        }
        // [text](url)
        if c == '[' {
            if let Some(close) = chars[i + 1..].iter().position(|&x| x == ']') {
                let after = i + 1 + close + 1;
                if chars.get(after) == Some(&'(') {
                    if let Some(paren) = chars[after + 1..].iter().position(|&x| x == ')') {
                        let label: String = chars[i + 1..i + 1 + close].iter().collect();
                        let url: String = chars[after + 1..after + 1 + paren].iter().collect();
                        if url.starts_with("http") {
                            flush(&mut buf, &mut items, bold, italic, false, None);
                            let mut label_buf = label;
                            flush(&mut label_buf, &mut items, bold, italic, false, Some(&url));
                            i = after + 1 + paren + 1;
                            continue;
                        }
                    }
                }
            }
        }
        buf.push(c);
        i += 1;
    }
    flush(&mut buf, &mut items, bold, italic, false, None);
    items
}

fn split_len(s: &str, max: usize) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    chars.chunks(max).map(|c| c.iter().collect()).collect()
}

pub fn paragraph(text: &str) -> Value {
    json!({ "object": "block", "type": "paragraph", "paragraph": { "rich_text": rich_text(text) } })
}

pub fn plain_paragraph(items: Vec<Value>) -> Value {
    json!({ "object": "block", "type": "paragraph", "paragraph": { "rich_text": items } })
}

pub fn heading(level: u8, text: &str, toggleable: bool) -> Value {
    let kind = format!("heading_{}", level.clamp(1, 3));
    json!({ "object": "block", "type": kind.clone(), kind: { "rich_text": rich_text(text), "is_toggleable": toggleable } })
}

pub fn divider() -> Value {
    json!({ "object": "block", "type": "divider", "divider": {} })
}

fn list_item(kind: &str, text: &str) -> Value {
    json!({ "object": "block", "type": kind, kind: { "rich_text": rich_text(text) } })
}

fn table(rows: &[Vec<String>]) -> Value {
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
    let children: Vec<Value> = rows
        .iter()
        .map(|row| {
            let cells: Vec<Value> = (0..width)
                .map(|i| Value::Array(rich_text(row.get(i).map(String::as_str).unwrap_or(""))))
                .collect();
            json!({ "object": "block", "type": "table_row", "table_row": { "cells": cells } })
        })
        .collect();
    json!({
        "object": "block",
        "type": "table",
        "table": { "table_width": width, "has_column_header": true, "has_row_header": false, "children": children }
    })
}

fn table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim().trim_start_matches('|').trim_end_matches('|');
    trimmed.split('|').map(|c| c.trim().to_string()).collect()
}

fn is_separator_row(line: &str) -> bool {
    let cells = table_cells(line);
    !cells.is_empty() && cells.iter().all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' '))
}

/// Markdown document to top-level Notion blocks.
pub fn markdown_to_blocks(markdown: &str) -> Vec<Value> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0;
    let mut paragraph_buf: Vec<String> = Vec::new();

    let flush_paragraph = |buf: &mut Vec<String>, blocks: &mut Vec<Value>| {
        if !buf.is_empty() {
            blocks.push(paragraph(&buf.join(" ")));
            buf.clear();
        }
    };

    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();
        if t.is_empty() {
            flush_paragraph(&mut paragraph_buf, &mut blocks);
            i += 1;
            continue;
        }
        if t.starts_with('|') {
            flush_paragraph(&mut paragraph_buf, &mut blocks);
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('|') {
                if !is_separator_row(lines[i]) {
                    rows.push(table_cells(lines[i]));
                }
                i += 1;
            }
            if !rows.is_empty() {
                blocks.push(table(&rows));
            }
            continue;
        }
        flush_paragraph_if_block(t, &mut paragraph_buf, &mut blocks);
        if let Some(rest) = t.strip_prefix("### ") {
            blocks.push(heading(3, rest, false));
        } else if let Some(rest) = t.strip_prefix("## ") {
            blocks.push(heading(2, rest, false));
        } else if let Some(rest) = t.strip_prefix("# ") {
            blocks.push(heading(1, rest, false));
        } else if t == "---" || t == "***" {
            blocks.push(divider());
        } else if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("• ")) {
            blocks.push(list_item("bulleted_list_item", rest));
        } else if let Some(rest) = numbered(t) {
            blocks.push(list_item("numbered_list_item", rest));
        } else if let Some(rest) = t.strip_prefix("> ") {
            blocks.push(json!({ "object": "block", "type": "quote", "quote": { "rich_text": rich_text(rest) } }));
        } else if t.starts_with("**") && t.ends_with("**") && t.len() > 4 && !t[2..t.len() - 2].contains("**") {
            // A bold-only line is a section title in the summary templates
            blocks.push(heading(3, &t[2..t.len() - 2], false));
        } else {
            paragraph_buf.push(t.to_string());
        }
        i += 1;
    }
    flush_paragraph(&mut paragraph_buf, &mut blocks);
    blocks
}

fn flush_paragraph_if_block(t: &str, buf: &mut Vec<String>, blocks: &mut Vec<Value>) {
    let starts_block = t.starts_with('#')
        || t.starts_with("- ")
        || t.starts_with("* ")
        || t.starts_with("• ")
        || t.starts_with("> ")
        || t == "---"
        || numbered(t).is_some()
        || (t.starts_with("**") && t.ends_with("**"));
    if starts_block && !buf.is_empty() {
        blocks.push(paragraph(&buf.join(" ")));
        buf.clear();
    }
}

fn numbered(t: &str) -> Option<&str> {
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && t[digits..].starts_with(". ") {
        Some(&t[digits + 2..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_formatting() {
        let items = rich_text("Le **budget** est *réduit* de `50 %`, voir [Notion](https://notion.so).");
        let texts: Vec<&str> = items.iter().map(|i| i["text"]["content"].as_str().unwrap()).collect();
        assert_eq!(texts, vec!["Le ", "budget", " est ", "réduit", " de ", "50 %", ", voir ", "Notion", "."]);
        assert_eq!(items[1]["annotations"]["bold"], true);
        assert_eq!(items[3]["annotations"]["italic"], true);
        assert_eq!(items[5]["annotations"]["code"], true);
        assert_eq!(items[7]["text"]["link"]["url"], "https://notion.so");
    }

    #[test]
    fn identifiers_with_underscores_are_not_italic() {
        let items = rich_text("fr_suivi_projet_client");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["annotations"]["italic"], false);
    }

    #[test]
    fn long_text_is_split() {
        let long = "a".repeat(4000);
        assert_eq!(rich_text(&long).len(), 3);
    }

    #[test]
    fn document_structure() {
        let md = "**Résumé exécutif**\n\nLe projet avance.\nSuite du paragraphe.\n\n- point 1\n- point **2**\n1. étape\n\n| **Responsable** | Action |\n| --- | --- |\n| Marie | Envoyer les accès |\n\n---\n## Suite";
        let blocks = markdown_to_blocks(md);
        let kinds: Vec<&str> = blocks.iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            vec!["heading_3", "paragraph", "bulleted_list_item", "bulleted_list_item", "numbered_list_item", "table", "divider", "heading_2"]
        );
        assert_eq!(blocks[1]["paragraph"]["rich_text"][0]["text"]["content"], "Le projet avance. Suite du paragraphe.");
        let table = &blocks[5]["table"];
        assert_eq!(table["table_width"], 2);
        assert_eq!(table["children"].as_array().unwrap().len(), 2, "separator row dropped");
    }
}
