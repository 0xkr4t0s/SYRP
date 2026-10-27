//! B1-B3 (and the B5 depth checks for the tree operations): the XML element model.

use syrp_var::xml::{XmlElement, XmlNode, XmlNodeView, is_valid_xml_name};
use syrp_var::{Diagnostic, Identifier};

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
