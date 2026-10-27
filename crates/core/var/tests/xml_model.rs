//! B1-B3 (and the B5 depth checks for the tree operations): the XML element model.

use std::cmp::Ordering;

use syrp_var::xml::{XmlElement, XmlNode, XmlNodeView, is_valid_xml_name};
use syrp_var::{Diagnostic, Identifier, Var};

type R = Result<(), syrp_var::VarError>;

fn el(tag: &str) -> XmlElement {
    XmlElement::new(tag).unwrap()
}

/// A chain of `depth` nested elements, built iteratively from the innermost one.
fn deep_chain(depth: usize) -> XmlElement {
    deep_chain_with_leaf(depth, el("leaf"))
}

fn deep_chain_with_leaf(depth: usize, mut inner: XmlElement) -> XmlElement {
    inner.add_text_element("bottom");
    for _ in 0..depth {
        let mut outer = el("n");
        outer.add_child_element(inner);
        inner = outer;
    }
    inner
}

#[test]
fn b1_names_follow_the_xml_name_production() {
    for good in [
        "a",
        "_",
        ":",
        "A1",
        "a-b.c_d:e",
        "é",
        "名前",
        "x\u{300}",
        "a·b",
    ] {
        assert!(is_valid_xml_name(good), "{good}");
        assert!(XmlElement::new(good).is_ok(), "{good}");
    }
    for bad in [
        "", "1a", "-a", ".a", "a b", "a<", "a&", "a\"", "a=b", "\u{300}x", "a\u{D7}",
    ] {
        assert!(!is_valid_xml_name(bad), "{bad:?}");
        let error = XmlElement::new(bad).unwrap_err();
        assert_eq!(error.code().as_str(), "SYRP-VAR-0004");
    }
}

#[test]
fn b1_set_tag_name_rejects_invalid_names_and_keeps_the_old_one() -> R {
    let mut e = el("a");
    assert_eq!(
        e.set_tag_name("b c").unwrap_err().code().as_str(),
        "SYRP-VAR-0004"
    );
    assert_eq!(e.tag_name(), "a");
    e.set_tag_name("b")?;
    assert_eq!(e.tag_name(), "b");
    Ok(())
}

#[test]
fn b1_children_are_elements_or_text_nodes() -> R {
    let mut root = el("r");
    root.add_text_element("t");
    root.create_new_child_element("c")?;
    let views: Vec<_> = root.children().iter().map(XmlNode::as_view).collect();
    assert!(matches!(views[0], XmlNodeView::Text("t")));
    assert!(matches!(views[1], XmlNodeView::Element(e) if e.tag_name() == "c"));
    let mut text = XmlNode::text("x");
    assert!(text.is_text_element() && text.set_text("y") && text.as_text() == Some("y"));
    let mut element = XmlNode::from(el("e"));
    assert!(!element.is_text_element() && !element.set_text("y"));
    assert_eq!(element.as_text(), None);
    Ok(())
}

#[test]
fn b1_tree_types_are_send_and_sync() {
    fn need<T: Send + Sync>() {}
    need::<XmlElement>();
    need::<XmlNode>();
    let tree = deep_chain(10);
    let back = std::thread::spawn(move || tree).join().unwrap();
    assert_eq!(back.all_sub_text(), "bottom");
}

#[test]
fn b2_set_attribute_replaces_in_place_and_appends_new_names() -> R {
    let mut e = el("a");
    e.set_attribute("x", "1")?;
    e.set_attribute("y", 2)?;
    e.set_attribute("z", 3_i64)?;
    e.set_attribute("x", true)?;
    let pairs: Vec<_> = e.attributes().map(|a| (a.name(), a.value())).collect();
    assert_eq!(pairs, [("x", "1"), ("y", "2"), ("z", "3")]);
    assert_eq!(e.num_attributes(), 3);
    assert_eq!(
        (e.attribute_name(2), e.attribute_value(2)),
        (Some("z"), Some("3"))
    );
    assert_eq!(e.attribute_name(3), None);
    assert_eq!(
        e.set_attribute("1x", 0).unwrap_err().code().as_str(),
        "SYRP-VAR-0004"
    );
    assert_eq!(e.num_attributes(), 3);
    Ok(())
}

#[test]
fn b2_names_are_case_sensitive_and_accept_identifiers() -> R {
    let mut e = el("a");
    let id = Identifier::new("Gain");
    e.set_attribute(id, 0.5)?;
    assert!(e.has_attribute(id) && e.has_attribute("Gain") && !e.has_attribute("gain"));
    assert_eq!(e.string_attribute(String::from("Gain")), Some("0.5"));
    assert!(e.remove_attribute(id) && !e.has_attribute("Gain"));
    e.set_attribute("a", 1)?;
    e.set_attribute("b", 2)?;
    assert_eq!(e.string_attribute_or("gain", "dflt"), "dflt");
    e.remove_all_attributes();
    assert_eq!(e.num_attributes(), 0);
    Ok(())
}

#[test]
fn b3_equivalence_compares_tags_attributes_and_children_in_order() -> R {
    let mut a = el("e");
    a.set_attribute("x", 1)?;
    a.set_attribute("y", 2)?;
    a.add_text_element("t");
    a.create_new_child_element("c")?;
    let mut b = a.clone();
    assert!(a.is_equivalent_to(&b, false) && a == b);
    b.remove_attribute("x");
    b.set_attribute("x", 1)?;
    assert!(!a.is_equivalent_to(&b, false) && a.is_equivalent_to(&b, true));
    let mut c = a.clone();
    c.child_mut(0).unwrap().set_text("t ");
    assert!(!a.is_equivalent_to(&c, true));
    let mut d = el("e");
    d.set_attribute("x", 1)?;
    d.set_attribute("y", 2)?;
    d.create_new_child_element("c")?;
    d.add_text_element("t");
    assert!(!a.is_equivalent_to(&d, true));
    let mut f = a.clone();
    f.set_attribute("y", 3)?;
    assert!(!a.is_equivalent_to(&f, true));
    let mut g = a.clone();
    g.set_tag_name("g")?;
    assert!(!a.is_equivalent_to(&g, true));
    Ok(())
}

#[test]
fn b5_clone_compare_and_drop_handle_100_000_levels() {
    let tree = deep_chain(100_000);
    let copy = tree.clone();
    assert!(tree.is_equivalent_to(&copy, false));
    assert_eq!(copy.all_sub_text(), "bottom");
    let mut leaf = el("leaf");
    leaf.set_attribute("changed", 1).unwrap();
    let other = deep_chain_with_leaf(100_000, leaf);
    assert!(!tree.is_equivalent_to(&other, false));
    assert!(
        format!("{tree:?}").contains("children: 1"),
        "Debug is shallow"
    );
    drop(tree);
    drop(copy);
    drop(other);
}

#[test]
fn b2_typed_readers_use_defaults_only_when_missing() -> R {
    let mut e = el("a");
    e.set_attribute("empty", "")?;
    assert_eq!(e.int_attribute("empty", 9), 0);
    assert_eq!(e.int_attribute("none", 9), 9);
    assert_eq!(e.double_attribute("none", 1.5), 1.5);
    assert!(!e.bool_attribute("empty", true));
    assert!(e.bool_attribute("none", true));
    Ok(())
}

#[test]
fn b2_conversions_agree_with_var_on_a_shared_table() -> R {
    let table = [
        "",
        "0",
        "1",
        "-1",
        "  42",
        "+7",
        "12px",
        "1.9",
        "-2.5e2dB",
        "1e3",
        "abc",
        "true",
        " TRUE ",
        "false",
        "yes",
        "0.0",
        "0.001",
        "99999999999",
        "-99999999999",
        ".5",
        "5.",
        "1e",
        "nan",
        "inf",
        "0x10",
    ];
    let mut e = el("a");
    for text in table {
        e.set_attribute("v", text)?;
        let var = Var::from(text);
        assert_eq!(e.int_attribute("v", 7), var.as_i32(), "int {text:?}");
        let (x, y) = (e.double_attribute("v", 7.0), var.as_f64());
        assert!(x == y || (x.is_nan() && y.is_nan()), "double {text:?}");
        assert_eq!(e.bool_attribute("v", false), var.as_bool(), "bool {text:?}");
    }
    Ok(())
}

#[test]
fn b2_f64_values_are_written_shortest_and_read_back_exactly() -> R {
    let mut e = el("a");
    for x in [
        0.1,
        2.0,
        -0.0,
        1e-300,
        1.7976931348623157e308,
        123_456.789,
        1.0 / 3.0,
    ] {
        e.set_attribute("x", x)?;
        assert_eq!(
            e.string_attribute("x"),
            Some(Var::from(x).to_text().as_str())
        );
        assert_eq!(
            e.double_attribute("x", f64::NAN).to_bits(),
            x.to_bits(),
            "{x}"
        );
    }
    e.set_attribute("x", 2.0)?;
    assert_eq!(e.string_attribute("x"), Some("2"));
    Ok(())
}

#[test]
fn b2_compare_attribute_with_and_without_case() -> R {
    let mut e = el("a");
    e.set_attribute("m", "ÄBC")?;
    assert!(e.compare_attribute("m", "äbc", true));
    assert!(!e.compare_attribute("m", "äbc", false));
    assert!(e.compare_attribute("m", "ÄBC", false));
    assert!(!e.compare_attribute("missing", "", false));
    Ok(())
}

#[test]
fn b3_insertion_positions() -> R {
    let mut e = el("r");
    e.add_child_element(el("b"));
    e.prepend_child_element(el("a"));
    e.insert_child_element(el("z"), 100);
    e.insert_child_element(XmlNode::text("t"), 1);
    let tags: Vec<_> = e
        .children()
        .iter()
        .map(|n| n.as_element().map_or("#text", XmlElement::tag_name))
        .collect();
    assert_eq!(tags, ["a", "#text", "b", "z"]);
    assert!(e.create_new_child_element("").is_err());
    assert_eq!(e.num_children(), 4);
    Ok(())
}

#[test]
fn b3_lookup_and_removal() -> R {
    let mut e = el("r");
    e.add_text_element("x");
    e.create_new_child_element("p")?.set_attribute("id", "a")?;
    e.create_new_child_element("q")?;
    e.create_new_child_element("p")?.set_attribute("id", "b")?;
    assert_eq!(e.child_elements().count(), 3);
    assert_eq!(
        e.child_elements()
            .next_back()
            .unwrap()
            .string_attribute("id"),
        Some("b")
    );
    assert_eq!(
        e.child_by_name("p").unwrap().string_attribute("id"),
        Some("a")
    );
    assert_eq!(e.child_by_attribute("id", "b").unwrap().tag_name(), "p");
    assert!(e.child_by_attribute("id", "c").is_none());
    let q = e.child_by_name("q").unwrap();
    assert!(e.contains_child(q));
    assert!(!e.contains_child(&q.clone()));
    assert!(e.child(0).unwrap().is_text_element() && e.child(4).is_none());
    e.delete_all_child_elements_with_tag_name("p");
    assert_eq!(e.num_children(), 2);
    assert_eq!(e.remove_child(0).unwrap().as_text(), Some("x"));
    assert!(e.remove_child(5).is_none());
    e.delete_all_child_elements();
    assert_eq!(e.num_children(), 0);
    Ok(())
}

#[test]
fn b3_replace_child_returns_the_old_node_or_gives_back_the_new_one() -> R {
    let mut e = el("r");
    e.add_text_element("old");
    let old = e.replace_child_element(0, el("new")).unwrap();
    assert_eq!(old.as_text(), Some("old"));
    assert_eq!(e.child(0).unwrap().as_element().unwrap().tag_name(), "new");
    let back = e.replace_child_element(1, XmlNode::text("x")).unwrap_err();
    assert_eq!(back.as_text(), Some("x"));
    Ok(())
}

#[test]
fn b3_namespace_helpers() {
    let e = el("svg:rect");
    assert_eq!(
        (e.namespace(), e.tag_name_without_namespace()),
        ("svg", "rect")
    );
    assert!(e.has_tag_name("svg:rect") && !e.has_tag_name("rect"));
    assert!(e.has_tag_name_ignoring_namespace("rect"));
    let plain = el("rect");
    assert_eq!(
        (plain.namespace(), plain.tag_name_without_namespace()),
        ("", "rect")
    );
    let two = el("a:b:c");
    assert_eq!(
        (two.namespace(), two.tag_name_without_namespace()),
        ("a", "b:c")
    );
}

#[test]
fn b3_sub_text_is_in_document_order() -> R {
    let mut p = el("p");
    p.add_text_element("1");
    let b = p.create_new_child_element("b")?;
    b.add_text_element("2");
    b.create_new_child_element("i")?.add_text_element("3");
    b.add_text_element("4");
    p.add_text_element("5");
    assert_eq!(p.all_sub_text(), "12345");
    assert_eq!(p.child_element_all_sub_text("b", "-"), "234");
    assert_eq!(p.child_element_all_sub_text("x", "-"), "-");
    Ok(())
}

#[test]
fn b3_sort_is_stable() -> R {
    let mut e = el("r");
    for (k, id) in [(2, "a"), (1, "b"), (2, "c"), (1, "d")] {
        let c = e.create_new_child_element("i")?;
        c.set_attribute("k", k)?;
        c.set_attribute("id", id)?;
    }
    let key = |n: &XmlNode| n.as_element().map_or(0, |e| e.int_attribute("k", 0));
    e.sort_child_elements(|a, b| key(a).cmp(&key(b)));
    let ids: Vec<_> = e
        .child_elements()
        .map(|c| c.string_attribute_or("id", ""))
        .collect();
    assert_eq!(ids, ["b", "d", "a", "c"]);
    e.sort_child_elements(|_, _| Ordering::Equal);
    assert_eq!(
        e.child_elements().next().unwrap().string_attribute("id"),
        Some("b")
    );
    Ok(())
}
