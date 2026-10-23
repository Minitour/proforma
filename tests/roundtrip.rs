//! Round-trip properties over every research case and the examples:
//! the writer's output must re-parse, and formatting must be idempotent.

use std::path::{Path, PathBuf};

use proforma::{encoding, parser, writer};

fn cases() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "pf") {
                out.push(p);
            }
        }
    }
    let mut v = Vec::new();
    walk(Path::new("research/oracle/cases"), &mut v);
    walk(Path::new("examples"), &mut v);
    v.sort();
    v
}

#[test]
fn writer_output_reparses_and_is_idempotent() {
    let mut checked = 0;
    for path in cases() {
        let src = encoding::decode(&std::fs::read(&path).unwrap()).0;
        let first = parser::parse(&src);
        if first.diagnostics.iter().any(|d| d.is_fatal()) {
            continue; // intentionally broken inputs
        }
        let w1 = writer::write(&first.doc);
        let second = parser::parse(&w1);
        let fatal: Vec<_> = second
            .diagnostics
            .iter()
            .filter(|d| d.is_fatal())
            .map(|d| d.render("written"))
            .collect();
        assert!(
            fatal.is_empty(),
            "{}: writer output does not parse: {:?}\n{}",
            path.display(),
            fatal,
            w1
        );
        let w2 = writer::write(&second.doc);
        assert_eq!(w1, w2, "{}: formatting is not idempotent", path.display());
        checked += 1;
    }
    assert!(checked > 80, "only {} cases checked", checked);
}

#[test]
fn examples_are_canonical_and_clean() {
    for path in cases().into_iter().filter(|p| p.starts_with("examples")) {
        let bytes = std::fs::read(&path).unwrap();
        let src = encoding::decode(&bytes).0;
        let out = parser::parse(&src);
        assert_eq!(
            writer::write(&out.doc),
            src,
            "{} is not in canonical form",
            path.display()
        );
        let diags = proforma::lint::lint(&out.doc, &out.diagnostics, &Default::default());
        assert!(diags.is_empty(), "{}: {:#?}", path.display(), diags);
    }
}
