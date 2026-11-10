use super::*;

const NS: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main""#;

fn rows(xml: &str, mode: Mode) -> Vec<RawRow> {
    let mut reader = SheetReader::new(xml.as_bytes(), mode);
    let mut out = Vec::new();
    while let Some(row) = reader.next_row().unwrap() {
        out.push(row);
    }
    out
}

/// `(reference, formula, body)` of every cell, with text resolved.
fn texts(row: &RawRow) -> Vec<(Reference, Option<&str>, Option<&str>)> {
    row.cells
        .iter()
        .map(|c| {
            let body = match &c.body {
                Body::Empty => None,
                Body::Inline(s) | Body::Value(s) => Some(row.text(s)),
            };
            (
                c.reference.clone(),
                c.formula.as_ref().map(|s| row.text(s)),
                body,
            )
        })
        .collect()
}

fn at(row: i64, column: i64) -> Reference {
    Reference::At { row, column }
}

#[test]
fn reads_values_formulas_and_inline_strings() {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><worksheet {NS}><sheetData>
        <row r="1"><c r="A1" s="2" t="n"><f>B1+1</f><v>1.0</v></c><c r="B1" t="inlineStr"><is><r><t>a</t></r><rPh><t>b</t></rPh></is></c></row>
        </sheetData></worksheet>"#
    );
    let got = rows(&xml, Mode::Document);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].cells[0].style, Style::Index(2));
    assert_eq!(got[0].cells[0].kind, Kind::ByFormat);
    assert_eq!(got[0].cells[1].kind, Kind::InlineStr);
    assert!(matches!(got[0].cells[1].body, Body::Inline(_)));
    assert_eq!(
        texts(&got[0]),
        vec![
            (at(1, 1), Some("B1+1"), Some("1.0")),
            (at(1, 2), None, Some("ab"))
        ]
    );
}

#[test]
fn keeps_attributes_ruby_has_to_read() {
    let xml = format!(
        r#"<worksheet {NS}><sheetData><row><c r="1A" s=" 3" t="e"/><c/></row></sheetData></worksheet>"#
    );
    let got = rows(&xml, Mode::Document);
    let cells = &got[0].cells;
    assert_eq!(cells[0].reference, Reference::Invalid("1A".into()));
    assert_eq!(cells[0].style, Style::Other(" 3".into()));
    assert_eq!(cells[0].kind, Kind::ByFormat);
    assert_eq!(cells[1].reference, Reference::Missing);
    assert_eq!(cells[1].style, Style::Index(0));

    let many: String = (0..12).map(|i| format!(" a{i}=\"\"")).collect();
    let xml = format!(
        "<worksheet {NS}><sheetData><row><c{many}\n r = 'B2'\ts=\"4\" t='s'/></row></sheetData></worksheet>"
    );
    let cell = &rows(&xml, Mode::Document)[0].cells[0];
    assert_eq!(cell.reference, at(2, 2));
    assert_eq!(cell.style, Style::Index(4));
    assert_eq!(cell.kind, Kind::Shared);
}

#[test]
fn resolves_references_and_normalizes_line_ends() {
    let xml = format!(
        "<worksheet {NS}><sheetData><row><c r=\"A1\" t=\"str\"><v>a&amp;b&#13;\r\nc<![CDATA[<d>]]></v></c></row></sheetData></worksheet>"
    );
    let got = rows(&xml, Mode::Document);
    assert_eq!(texts(&got[0])[0].2, Some("a&b\r\nc<d>"));
}

#[test]
fn first_value_wins_and_formula_after_it_is_ignored() {
    let xml = format!(
        "<worksheet {NS}><sheetData><row><c r=\"A1\"><v>1</v><f>X</f><v>2</v></c><c r=\"B1\"><f>Y</f></c></row></sheetData></worksheet>"
    );
    let got = rows(&xml, Mode::Document);
    assert_eq!(
        texts(&got[0]),
        vec![(at(1, 1), None, Some("1")), (at(1, 2), None, None)]
    );
}

#[test]
fn empty_inline_string_does_not_decide_the_value() {
    let xml = format!(
        "<worksheet {NS}><sheetData><row><c r=\"A1\" t=\"inlineStr\"><is><t></t></is><v>7</v></c></row></sheetData></worksheet>"
    );
    let got = rows(&xml, Mode::Document);
    assert!(matches!(got[0].cells[0].body, Body::Value(_)));
    assert_eq!(texts(&got[0])[0].2, Some("7"));
}

#[test]
fn document_mode_reads_only_sheet_data_rows_and_c_children() {
    let xml = format!(
        "<worksheet {NS}><sheetPr><row/></sheetPr><sheetData><row r=\"1\"/><row r=\"2\"><x>1</x><c r=\"A2\"/></row></sheetData></worksheet>"
    );
    let got = rows(&xml, Mode::Document);
    assert_eq!(got.len(), 2);
    assert!(got[0].cells.is_empty());
    assert_eq!(texts(&got[1]), vec![(at(2, 1), None, None)]);
}

#[test]
fn document_mode_matches_prefixed_names_by_local_name() {
    let xml = r#"<x:worksheet xmlns:x="urn:x"><x:sheetData><x:row><x:c r="A1"><x:v>3</x:v></x:c></x:row></x:sheetData></x:worksheet>"#;
    let got = rows(xml, Mode::Document);
    assert_eq!(texts(&got[0])[0].2, Some("3"));
}

#[test]
fn stream_mode_needs_an_unprefixed_row_and_takes_every_child_element() {
    let prefixed = r#"<x:worksheet xmlns:x="urn:x"><x:sheetData><x:row><x:c r="A1"/></x:row></x:sheetData></x:worksheet>"#;
    assert!(rows(prefixed, Mode::Stream).is_empty());

    let xml = format!(
        "<worksheet {NS}><sheetData><row><c r=\"A1\"/><extra r=\"B1\"><v>1</v></extra></row></sheetData></worksheet>"
    );
    let got = rows(&xml, Mode::Stream);
    assert_eq!(
        texts(&got[0]),
        vec![(at(1, 1), None, None), (at(1, 2), None, Some("1"))]
    );
}

#[test]
fn rejects_inputs_it_cannot_reproduce() {
    let latin1 = r#"<?xml version="1.0" encoding="ISO-8859-1"?><worksheet/>"#;
    assert!(SheetReader::new(latin1.as_bytes(), Mode::Document)
        .next_row()
        .is_err());
    for body in [
        "<v>&custom;</v>",
        "<v>a&#1;b</v>",
        "<v>a\u{1}b</v>",
        "<v>\u{FFFE}</v>",
    ] {
        let xml =
            format!("<worksheet {NS}><sheetData><row><c>{body}</c></row></sheetData></worksheet>");
        assert!(
            SheetReader::new(xml.as_bytes(), Mode::Document)
                .next_row()
                .is_err(),
            "{body:?}"
        );
    }
    let attribute =
        format!(r#"<worksheet {NS}><sheetData><row><c r="A&#2;1"/></row></sheetData></worksheet>"#);
    assert!(SheetReader::new(attribute.as_bytes(), Mode::Document)
        .next_row()
        .is_err());
    let truncated = format!("<worksheet {NS}><sheetData><row><c><v>1</v></c></row>");
    let mut reader = SheetReader::new(truncated.as_bytes(), Mode::Document);
    assert!(reader.next_row().unwrap().is_some());
    assert!(
        reader.next_row().is_err(),
        "a sheet that ends with open elements"
    );
}

#[test]
fn rejects_xml_that_is_not_well_formed() {
    let rows = r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#;
    for (label, xml) in [
        (
            "unquoted value",
            "<worksheet><sheetData><row r=1/></sheetData></worksheet>".to_string(),
        ),
        (
            "no space between attributes",
            r#"<worksheet><sheetData><row r="1"s="2"/></sheetData></worksheet>"#.to_string(),
        ),
        (
            "`<` in a value",
            format!(r#"<worksheet x="a<b">{rows}</worksheet>"#),
        ),
        (
            "unknown entity in a value",
            format!(r#"<worksheet x="&bogus;">{rows}</worksheet>"#),
        ),
        (
            "bare `&` in a value",
            format!(r#"<worksheet x="a & b">{rows}</worksheet>"#),
        ),
        (
            "`]]>` in text",
            format!("<worksheet>{rows}a ]]> b</worksheet>"),
        ),
        (
            "second root",
            format!("<worksheet>{rows}</worksheet><worksheet/>"),
        ),
        (
            "text before the root",
            format!("x<worksheet>{rows}</worksheet>"),
        ),
        (
            "doctype",
            format!("<!DOCTYPE worksheet><worksheet>{rows}</worksheet>"),
        ),
        (
            "illegal character in a PI",
            format!("<?pi \u{1}?><worksheet>{rows}</worksheet>"),
        ),
        (
            "`--` in a comment",
            format!("<!-- a -- b --><worksheet>{rows}</worksheet>"),
        ),
        (
            "invalid name",
            format!("<worksheet><1x/>{rows}</worksheet>"),
        ),
        (
            "late declaration",
            format!(r#"<!-- c --><?xml version="1.0"?><worksheet>{rows}</worksheet>"#),
        ),
        ("empty document", String::new()),
        (
            "PI in a value",
            "<worksheet><sheetData><row><c><v>1<? x?></v></c></row></sheetData></worksheet>"
                .to_string(),
        ),
        (
            "non-ASCII attribute name",
            "<worksheet><sheetData><row><c \u{d7}=\"1\"/></row></sheetData></worksheet>"
                .to_string(),
        ),
        (
            "non-ASCII element name",
            "<worksheet><sheetData><row><c><v>1<\u{d7}/>2</v></c></row></sheetData></worksheet>"
                .to_string(),
        ),
        (
            "undeclared element prefix",
            format!("<worksheet><sheetData><x:row>{rows}</x:row></sheetData></worksheet>"),
        ),
        (
            "empty prefix",
            "<worksheet><sheetData><:row/></sheetData></worksheet>".to_string(),
        ),
        (
            "undeclared value prefix",
            "<worksheet><sheetData><row><c><x:v>1</x:v><v>9</v></c></row></sheetData></worksheet>"
                .to_string(),
        ),
    ] {
        for mode in [Mode::Document, Mode::Stream] {
            let mut reader = SheetReader::new(xml.as_bytes(), mode);
            let read: Result<Vec<RawRow>> =
                std::iter::from_fn(|| reader.next_row().transpose()).collect();
            assert!(read.is_err(), "{label} ({mode:?})");
        }
    }
    for (label, xml) in [
        (
            "prefixed r",
            r#"<worksheet xmlns:x="urn:x"><sheetData><row><c x:r="B9" r="A1"><v>1</v></c></row></sheetData></worksheet>"#,
        ),
        (
            "prefixed t",
            r#"<worksheet xmlns:x="urn:x"><sheetData><row><c r="A1" x:t="str"><v>1</v></c></row></sheetData></worksheet>"#,
        ),
    ] {
        assert!(
            SheetReader::new(xml.as_bytes(), Mode::Document)
                .next_rows(9)
                .is_err(),
            "{label}"
        );
        assert!(
            SheetReader::new(xml.as_bytes(), Mode::Stream)
                .next_rows(9)
                .is_ok(),
            "{label} streams"
        );
    }
    for (label, xml) in [
        (
            "comment in a row",
            "<worksheet><sheetData><row><!-- c --><c r=\"A1\"/></row></sheetData></worksheet>",
        ),
        (
            "text in a row",
            "<worksheet><sheetData><row>x<c r=\"A1\"/></row></sheetData></worksheet>",
        ),
    ] {
        assert!(
            SheetReader::new(xml.as_bytes(), Mode::Stream)
                .next_rows(9)
                .is_err(),
            "{label}"
        );
        assert!(
            SheetReader::new(xml.as_bytes(), Mode::Document)
                .next_rows(9)
                .is_ok(),
            "{label} in a document"
        );
    }
    for (label, xml) in [
        ("nested t", "<worksheet><sheetData><row><c t=\"inlineStr\"><is><t>a<t>b</t>c</t></is></c></row></sheetData></worksheet>"),
        ("empty prefix namespace", "<worksheet><sheetData><row r=\"1\" xmlns:x=\"\"><x:c r=\"A1\"><v>1</v></x:c></row></sheetData></worksheet>"),
        ("duplicated attribute", "<worksheet><sheetData><row r=\"1\" r=\"2\"><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>"),
        ("duplicated attribute among many", "<worksheet><sheetData><row a1=\"\" a2=\"\" a3=\"\" a4=\"\" a5=\"\" a6=\"\" a7=\"\" a8=\"\" a9=\"\" a2=\"\"><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>"),
        ("attribute without a value", "<worksheet><sheetData><row r><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>"),
        ("attribute without `=`", "<worksheet><sheetData><row r \"1\"><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>"),
        ("attribute without a name", "<worksheet><sheetData><row =\"1\"><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>"),
        ("`/` before the end of a tag", "<worksheet><sheetData><row r=\"1\" / ><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>"),
        ("attribute name with two prefixes", "<worksheet xmlns:a=\"urn:a\"><sheetData><row a:b:c=\"1\"><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>"),
        ("unknown entity in skipped content", "<worksheet><sheetData><row><c r=\"A1\"><v>1</v></c></row></sheetData><ext>&bogus;</ext></worksheet>"),
    ] {
        for mode in [Mode::Document, Mode::Stream] {
            let mut reader = SheetReader::new(xml.as_bytes(), mode);
            let read: Result<Vec<RawRow>> = std::iter::from_fn(|| reader.next_row().transpose()).collect();
            assert!(read.is_err(), "{label} ({mode:?})");
        }
    }
    for (label, xml) in [
        ("nested row", "<worksheet><sheetData><row r=\"1\"><c r=\"A1\"><ext><row r=\"2\"/></ext></c></row></sheetData></worksheet>"),
        ("prefixed t in a streamed row", "<worksheet><sheetData><row><c t=\"inlineStr\"><is><x:t xmlns:x=\"urn:other\">hi</x:t></is></c></row></sheetData></worksheet>"),
        ("preserved whitespace in a row", "<worksheet><sheetData><row r=\"1\" xml:space=\"preserve\"> <c r=\"A1\"/> </row></sheetData></worksheet>"),
    ] {
        assert!(SheetReader::new(xml.as_bytes(), Mode::Stream).next_rows(9).is_err(), "{label}");
        assert!(SheetReader::new(xml.as_bytes(), Mode::Document).next_rows(9).is_ok(), "{label} in a document");
    }
    let declared = r#"<x:worksheet xmlns:x="urn:x"><x:sheetData><x:row><x:c r="A1"><x:v>3</x:v></x:c></x:row></x:sheetData></x:worksheet>"#;
    assert_eq!(
        SheetReader::new(declared.as_bytes(), Mode::Document)
            .next_rows(9)
            .unwrap()
            .len(),
        1
    );
    let scoped = r#"<worksheet><a xmlns:x="urn:x"/><sheetData><x:row/></sheetData></worksheet>"#;
    assert!(
        SheetReader::new(scoped.as_bytes(), Mode::Document)
            .next_rows(9)
            .is_err(),
        "a prefix out of scope"
    );
    let fine = format!(
        r#"<?xml version="1.0"?>
<!-- c --><?pi ok?><worksheet x="a &amp; b" y='1'>{rows}</worksheet>
"#
    );
    let mut reader = SheetReader::new(fine.as_bytes(), Mode::Document);
    assert_eq!(reader.next_rows(10).unwrap().len(), 1);
}

#[test]
fn returns_the_rows_before_an_error_first() {
    let xml = format!(
        "<worksheet {NS}><sheetData><row/><row><c><v>&x;</v></c></row></sheetData></worksheet>"
    );
    let mut reader = SheetReader::new(xml.as_bytes(), Mode::Document);
    assert_eq!(reader.next_rows(5).unwrap().len(), 1);
    assert!(reader.next_rows(5).is_err());
}

#[test]
fn batches_rows() {
    let xml = format!("<worksheet {NS}><sheetData><row/><row/><row/></sheetData></worksheet>");
    let mut reader = SheetReader::new(xml.as_bytes(), Mode::Document);
    assert_eq!(reader.next_rows(2).unwrap().len(), 2);
    assert_eq!(reader.next_rows(2).unwrap().len(), 1);
    assert!(reader.next_rows(2).unwrap().is_empty());

    let xml = format!(
        "<worksheet {NS}><sheetData><row><c/><c/><c/></row><row><c/></row></sheetData></worksheet>"
    );
    let mut reader = SheetReader::new(xml.as_bytes(), Mode::Document);
    assert_eq!(
        reader.next_rows(2).unwrap().len(),
        1,
        "a batch ends once it holds enough cells"
    );
}

#[test]
fn parses_references_like_roo() {
    assert_eq!(parse_reference("A1"), Some((1, 1)));
    assert_eq!(parse_reference("aa10"), Some((10, 27)));
    assert_eq!(parse_reference("XFD1048576"), Some((1048576, 16384)));
    assert_eq!(parse_reference("1A"), None);
    assert_eq!(parse_reference("A"), None);
    assert_eq!(parse_reference("A1B"), None);
    assert_eq!(parse_reference("$A$1"), None);
}

#[test]
fn parses_only_plain_indexes() {
    assert_eq!(parse_index("12"), Some(12));
    assert_eq!(parse_index("007"), Some(7));
    assert_eq!(parse_index(""), None);
    assert_eq!(parse_index("+1"), None);
    assert_eq!(parse_index(" 1"), None);
}
