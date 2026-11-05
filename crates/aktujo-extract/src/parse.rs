// SPDX-License-Identifier: MIT
// Taken over from notenwart (crates/notenwart-app/src/recognize.rs, AGPL-3.0-or-later); published here under MIT by
// its copyright holder.
//! Parsers of what the external programs write: `pdftotext -bbox-layout` (XHTML with boxes in points) and Tesseract's
//! TSV. Pure functions, so that they are there without the feature `external` – a program that runs the tools its
//! own way can use them.

use crate::{Line, Method, Page};

fn attribute(tag: &str, name: &str) -> Option<f32> {
    let start = tag.find(&format!("{name}=\""))? + name.len() + 2;
    let end = tag[start..].find('"')? + start;
    tag[start..end].parse().ok()
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Every page of `pdftotext -bbox-layout` output, with its size in points and its lines; their method is the text
/// layer.
pub fn pdftotext_bbox(xhtml: &str) -> Vec<Page> {
    let mut pages = vec![];
    let mut rest = xhtml;
    while let Some(start) = rest.find("<page ") {
        let tag_end = rest[start..].find('>').map_or(rest.len(), |e| start + e);
        let end = rest[tag_end..].find("</page>").map_or(rest.len(), |e| tag_end + e);
        let tag = &rest[start..tag_end];
        if let (Some(width), Some(height)) = (attribute(tag, "width"), attribute(tag, "height")) {
            let lines = page_lines(&rest[tag_end..end], width, height);
            pages.push(Page { width, height, lines, method: Method::TextLayer });
        }
        rest = &rest[end..];
        if rest.starts_with("</page>") {
            rest = &rest[7..];
        }
    }
    pages
}

fn page_lines(xhtml: &str, width: f32, height: f32) -> Vec<Line> {
    let mut lines = vec![];
    let mut rest = xhtml;
    while let Some(start) = rest.find("<line ") {
        let Some(end) = rest[start..].find("</line>").map(|e| start + e) else {
            break;
        };
        let block = &rest[start..end];
        let tag = &block[..block.find('>').unwrap_or(block.len())];
        let mut text = vec![];
        let mut words = block;
        while let Some(w) = words.find("<word ") {
            let open = words[w..].find('>').map_or(words.len(), |e| w + e + 1);
            let close = words[open..].find("</word>").map_or(words.len(), |e| open + e);
            text.push(unescape(&words[open..close]));
            words = &words[close..];
        }
        if let (Some(x0), Some(y0), Some(x1), Some(y1)) =
            (attribute(tag, "xMin"), attribute(tag, "yMin"), attribute(tag, "xMax"), attribute(tag, "yMax"))
        {
            lines.push(Line { text: text.join(" "), x0: x0 / width, y0: y0 / height, x1: x1 / width, y1: y1 / height });
        }
        rest = &rest[end + 7..];
    }
    lines
}

/// A page from Tesseract's TSV output: words (level 5) grouped by block, paragraph and line, words with a confidence
/// below `min_confidence` dropped; its size in pixels from the page row (level 1). The method is OCR.
pub fn tesseract_tsv(tsv: &str, min_confidence: f32) -> Page {
    let mut size = (1.0f32, 1.0f32);
    let mut lines: Vec<((u32, u32, u32), Line)> = vec![];
    for row in tsv.lines().skip(1) {
        let cols: Vec<&str> = row.split('\t').collect();
        if cols.len() < 12 {
            continue;
        }
        let n = |i: usize| cols[i].parse::<f32>().unwrap_or(0.0);
        match cols[0] {
            "1" => size = (n(8).max(1.0), n(9).max(1.0)),
            "5" => {
                let text = cols[11].trim();
                // Lines, stamps and dust come out as short garbage with low confidence.
                if text.is_empty() || n(10) < min_confidence {
                    continue;
                }
                let key = (n(2) as u32, n(3) as u32, n(4) as u32);
                let (x0, y0) = (n(6) / size.0, n(7) / size.1);
                let (x1, y1) = ((n(6) + n(8)) / size.0, (n(7) + n(9)) / size.1);
                match lines.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, line)) => {
                        line.text.push(' ');
                        line.text.push_str(text);
                        line.x0 = line.x0.min(x0);
                        line.y0 = line.y0.min(y0);
                        line.x1 = line.x1.max(x1);
                        line.y1 = line.y1.max(y1);
                    }
                    None => lines.push((key, Line { text: text.to_string(), x0, y0, x1, y1 })),
                }
            }
            _ => {}
        }
    }
    let mut lines: Vec<Line> = lines.into_iter().map(|(_, l)| l).collect();
    lines.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
    Page { width: size.0, height: size.1, lines, method: Method::Ocr }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, x0: f32, y0: f32, x1: f32, y1: f32) -> Line {
        Line { text: text.into(), x0, y0, x1, y1 }
    }

    #[test]
    fn parses_pdftotext_and_tesseract_output() {
        let xhtml = r#"<doc><page width="600.000000" height="800.000000"><flow><block>
            <line xMin="60" yMin="80" xMax="300" yMax="120"><word xMin="60" yMin="80" xMax="150" yMax="120">Rock</word>
            <word xMin="160" yMin="80" xMax="300" yMax="120">&amp; Roll</word></line></block></flow></page></doc>"#;
        let pages = pdftotext_bbox(xhtml);
        assert_eq!(pages[0].lines, vec![line("Rock & Roll", 0.1, 0.1, 0.5, 0.15)]);
        assert_eq!((pages[0].width, pages[0].height, pages[0].method), (600.0, 800.0, Method::TextLayer));

        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
                   1\t1\t0\t0\t0\t0\t0\t0\t1000\t2000\t-1\t\n\
                   5\t1\t1\t1\t1\t1\t100\t200\t200\t100\t91\tStille\n\
                   5\t1\t1\t1\t1\t2\t320\t200\t180\t100\t89\tNacht\n\
                   5\t1\t2\t1\t1\t1\t500\t900\t20\t20\t12\t~\n";
        let page = tesseract_tsv(tsv, 55.0);
        assert_eq!(page.lines, vec![line("Stille Nacht", 0.1, 0.1, 0.5, 0.15)]);
        assert_eq!((page.width, page.height, page.method), (1000.0, 2000.0, Method::Ocr));
        assert_eq!(tesseract_tsv(tsv, 0.0).lines.len(), 2);
    }

    #[test]
    fn reads_every_page_of_pdftotext_output() {
        let xhtml = r#"<doc><page width="100" height="200"><line xMin="10" yMin="20" xMax="50" yMax="30"><word xMin="10">A</word></line></page>
            <page width="100" height="200"></page><page width="100" height="200"><line xMin="0" yMin="0" xMax="10" yMax="10"><word xMin="0">C</word></line></page></doc>"#;
        let pages = pdftotext_bbox(xhtml);
        assert_eq!(pages.iter().map(|p| p.lines.len()).collect::<Vec<_>>(), vec![1, 0, 1]);
        assert_eq!(pages[2].lines[0].text, "C");
    }
}
