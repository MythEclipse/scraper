//! Regression tests for html5ever's foster-parenting of table content.
//!
//! Text nodes that appear inside `<table>` but outside a cell are foster
//! parented into the enclosing `<body>` rather than staying in the table. The
//! scraper's HTML parsers depend on that behaviour: several source sites emit
//! malformed tables, and a parser that silently dropped the fostered text would
//! lose real content without erroring.
//!
//! These assertions run on every `cargo test` — they previously lived in a
//! `main()`-based binary that only executed when run by hand.

use scraper::Selector;
use scraper_service::infrastructure::scraping::parsing_utils::parse_html;

const FIXTURE: &str = include_str!("fixtures/foster_parenting_minimal.html");

fn body_text(html: &str) -> String {
    let document = parse_html(html);
    let body = Selector::parse("body").expect("valid selector");
    document
        .select(&body)
        .next()
        .map(|el| el.text().collect())
        .unwrap_or_default()
}

#[test]
fn text_outside_a_cell_is_fostered_into_body() {
    let text = body_text(FIXTURE);
    assert!(
        text.contains("orphaned text"),
        "expected 'orphaned text' fostered out of the table, got: {text:?}"
    );
    assert!(
        text.contains("more text"),
        "expected 'more text' fostered out of the table, got: {text:?}"
    );
    assert!(
        text.contains("cell content"),
        "cell content must survive parsing, got: {text:?}"
    );
}

#[test]
fn fostered_text_does_not_disturb_cell_structure() {
    let document = parse_html(FIXTURE);
    let count = |selector: &str| {
        let sel = Selector::parse(selector).expect("valid selector");
        document.select(&sel).count()
    };

    assert_eq!(count("table"), 1, "exactly one table expected");
    assert_eq!(count("tr"), 1, "exactly one row expected");
    assert_eq!(count("td"), 1, "exactly one cell expected");

    let cell = Selector::parse("td").expect("valid selector");
    let cell_text: String = document
        .select(&cell)
        .next()
        .map(|el| el.text().collect())
        .unwrap_or_default();
    assert_eq!(
        cell_text.trim(),
        "cell content",
        "the cell keeps its own text; only the stray text is fostered"
    );
}

#[test]
fn parser_output_retains_every_text_node() {
    let text = body_text(FIXTURE);
    for fragment in ["orphaned text", "more text", "cell content"] {
        assert!(
            text.contains(fragment),
            "parser dropped {fragment:?}; full body text was {text:?}"
        );
    }
}
