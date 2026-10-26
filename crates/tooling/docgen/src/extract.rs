//! Walks one crate's rustdoc JSON from its root module and collects the public items.

use std::collections::HashSet;

use rustdoc_types::{Attribute, Function as E2, GenericBound, Visibility};
use rustdoc_types::{Crate, Id, Item, ItemEnum as E, MacroKind, StructKind, VariantKind};

use crate::render::{bounds, generic_params as gp, header, sig_tail, ty, where_clause as wc, wrap};
use crate::{ApiEntry, ItemKind, SourceLocation};

/// Turns one crate's rustdoc JSON into index entries (B1), unsorted.
///
/// Walks from the root module through public modules, `pub use` re-exports, inherent `impl`
/// blocks, trait items, enum variants and named struct fields. Trait impls are left out:
/// they document the trait, not the type.
///
/// ```no_run
/// let krate = syrp_docgen::load_rustdoc_json("target/doc/syrp_core.json".as_ref()).unwrap();
/// for entry in syrp_docgen::index_crate(&krate) {
///     println!("{} {}", entry.path, entry.signature);
/// }
/// ```
pub fn index_crate(krate: &Crate) -> Vec<ApiEntry> {
    let (crate_name, out, open) = (String::new(), Vec::new(), HashSet::new());
    let mut w = Walker {
        krate,
        crate_name,
        out,
        open,
    };
    if let Some(root) = krate.index.get(&krate.root) {
        w.crate_name = root.name.clone().unwrap_or_default();
        w.module_items(root, &w.crate_name.clone());
    }
    w.out
}

struct Walker<'a> {
    krate: &'a Crate,
    crate_name: String,
    out: Vec<ApiEntry>,
    /// Modules being walked, so a glob re-export cycle cannot recurse forever.
    open: HashSet<Id>,
}

fn public(item: &Item) -> bool {
    item.visibility == Visibility::Public
}

impl<'a> Walker<'a> {
    fn get(&self, id: &Id) -> Option<&'a Item> {
        self.krate.index.get(id)
    }

    fn module_items(&mut self, module: &Item, prefix: &str) {
        let krate = self.krate;
        if let E::Module(m) = &module.inner
            && self.open.insert(module.id)
        {
            let items = m.items.iter().filter_map(|id| krate.index.get(id));
            for item in items.filter(|i| public(i)) {
                self.visit(item, prefix, None);
            }
            self.open.remove(&module.id);
        }
    }

    /// Visits a public item reachable as `prefix::<name>`, where `rename` overrides the name.
    fn visit(&mut self, item: &Item, prefix: &str, rename: Option<&str>) {
        if let E::Use(u) = &item.inner {
            match u.id.as_ref().and_then(|id| self.get(id)) {
                Some(target) if u.is_glob => self.module_items(target, prefix),
                Some(target) => self.visit(target, prefix, Some(&u.name)),
                None => {}
            }
            return;
        }
        let (Some(name), Some(kind)) =
            (rename.or(item.name.as_deref()), kind_of(&item.inner, false))
        else {
            return;
        };
        let path = format!("{prefix}::{name}");
        self.push(item, &path, name, kind);
        match &item.inner {
            E::Module(_) => self.module_items(item, &path),
            E::Struct(s) => {
                if let StructKind::Plain { fields, .. } = &s.kind {
                    self.members(fields, &path, false, true);
                }
                self.inherent_impls(&s.impls, &path);
            }
            E::Union(u) => {
                self.members(&u.fields, &path, false, true);
                self.inherent_impls(&u.impls, &path);
            }
            E::Enum(e) => {
                self.members(&e.variants, &path, false, false);
                self.inherent_impls(&e.impls, &path);
            }
            E::Trait(t) => self.members(&t.items, &path, true, false),
            _ => {}
        }
    }

    fn inherent_impls(&mut self, impls: &[Id], parent: &str) {
        let krate = self.krate;
        for item in impls.iter().filter_map(|id| krate.index.get(id)) {
            if let E::Impl(i) = &item.inner
                && i.trait_.is_none()
                && i.blanket_impl.is_none()
                && !i.is_synthetic
            {
                self.members(&i.items, parent, true, true);
            }
        }
    }

    /// Pushes variants, fields or associated items (`assoc`), only public ones if `pub_only`.
    fn members(&mut self, ids: &[Id], parent: &str, assoc: bool, pub_only: bool) {
        let krate = self.krate;
        for item in ids.iter().filter_map(|id| krate.index.get(id)) {
            if let (Some(name), Some(kind)) = (item.name.as_deref(), kind_of(&item.inner, assoc))
                && (public(item) || !pub_only)
            {
                self.push(item, &format!("{parent}::{name}"), name, kind);
            }
        }
    }

    fn push(&mut self, item: &Item, path: &str, name: &str, kind: ItemKind) {
        let deprecated = item
            .deprecation
            .as_ref()
            .map(|d| match (&d.note, &d.since) {
                (Some(note), _) => note.clone(),
                (None, since) => {
                    "deprecated".to_owned()
                        + &wrap(" since ", since.clone().unwrap_or_default(), "")
                }
            });
        let source = item.span.as_ref().map(|s| SourceLocation {
            file: s.filename.to_string_lossy().replace('\\', "/"),
            line: s.begin.0,
        });
        self.out.push(ApiEntry {
            crate_name: self.crate_name.clone(),
            path: path.to_owned(),
            kind,
            signature: self.signature(item, name),
            summary: first_paragraph(item.docs.as_deref().unwrap_or("")),
            aliases: aliases(&item.attrs),
            deprecated,
            source,
        });
    }

    fn signature(&self, item: &Item, name: &str) -> String {
        let vis = if public(item) { "pub " } else { "" };
        let generics = match &item.inner {
            E::Function(E2 { generics, .. }) => Some(generics),
            E::Struct(s) => Some(&s.generics),
            E::Union(u) => Some(&u.generics),
            E::Enum(e) => Some(&e.generics),
            E::Trait(t) => Some(&t.generics),
            E::TraitAlias(t) => Some(&t.generics),
            E::TypeAlias(t) => Some(&t.generics),
            E::AssocType { generics, .. } => Some(generics),
            _ => None,
        };
        let (g, w) = generics.map(|g| (gp(g), wc(g))).unwrap_or_default();
        let opt = |pre: &str, s: Option<String>| wrap(pre, s.unwrap_or_default(), "");
        let colon_bounds = |b: &[GenericBound]| wrap(": ", bounds(b), "");
        match &item.inner {
            E::Module(_) => format!("{vis}mod {name}"),
            E::Function(f) => {
                let (h, tail) = (header(&f.header), sig_tail(&f.sig, false));
                format!("{vis}{h}fn {name}{g}{tail}{w}")
            }
            E::Struct(s) => {
                let tuple = match &s.kind {
                    StructKind::Tuple(fields) => self.tuple_fields(fields),
                    _ => String::new(),
                };
                format!("{vis}struct {name}{g}{tuple}{w}")
            }
            E::Union(_) => format!("{vis}union {name}{g}{w}"),
            E::Enum(_) => format!("{vis}enum {name}{g}{w}"),
            E::Variant(v) => {
                let body = match &v.kind {
                    VariantKind::Plain => String::new(),
                    VariantKind::Tuple(fields) => self.tuple_fields(fields),
                    VariantKind::Struct { fields, .. } => {
                        let named = fields.iter().filter_map(|id| self.get(id));
                        let named = named.map(|f| match (&f.inner, &f.name) {
                            (E::StructField(t), Some(n)) => format!("{n}: {}", ty(t)),
                            _ => "..".to_owned(),
                        });
                        format!(" {{ {} }}", named.collect::<Vec<_>>().join(", "))
                    }
                };
                let disc = opt(" = ", v.discriminant.as_ref().map(|d| d.expr.clone()));
                format!("{name}{body}{disc}")
            }
            E::StructField(t) => format!("{vis}{name}: {}", ty(t)),
            E::Trait(t) => {
                let u = if t.is_unsafe { "unsafe " } else { "" };
                let a = if t.is_auto { "auto " } else { "" };
                let sup = colon_bounds(&t.bounds);
                format!("{vis}{u}{a}trait {name}{g}{sup}{w}")
            }
            E::TraitAlias(t) => format!("{vis}trait {name}{g} = {}", bounds(&t.params)),
            E::TypeAlias(t) => format!("{vis}type {name}{g}{w} = {}", ty(&t.type_)),
            E::Constant { type_, const_ } => {
                format!("{vis}const {name}: {} = {}", ty(type_), const_.expr)
            }
            E::Static(s) => {
                let m = if s.is_mutable { "mut " } else { "" };
                format!("{vis}static {m}{name}: {}", ty(&s.type_))
            }
            E::AssocConst { type_, value, .. } => {
                let value = opt(" = ", value.clone());
                format!("{vis}const {name}: {}{value}", ty(type_))
            }
            E::AssocType {
                bounds: b, type_, ..
            } => {
                let (b, default) = (colon_bounds(b), opt(" = ", type_.as_ref().map(ty)));
                format!("{vis}type {name}{g}{b}{default}")
            }
            E::Macro(_) => format!("macro_rules! {name}"),
            E::ProcMacro(p) => match p.kind {
                MacroKind::Bang => format!("{name}!(...)"),
                MacroKind::Attr => format!("#[{name}]"),
                MacroKind::Derive => format!("#[derive({name})]"),
            },
            _ => format!("{vis}type {name}"), // ExternType; other kinds are never indexed.
        }
    }

    /// `(pub T, U, _)`: tuple fields with their visibility, `_` for stripped ones.
    fn tuple_fields(&self, fields: &[Option<Id>]) -> String {
        let field = |f: &Option<Id>| match f.as_ref().and_then(|id| self.get(id)) {
            Some(item) if public(item) => format!("pub {}", field_ty(item)),
            Some(item) => field_ty(item),
            None => "_".to_owned(),
        };
        format!(
            "({})",
            fields.iter().map(field).collect::<Vec<_>>().join(", ")
        )
    }
}

fn field_ty(item: &Item) -> String {
    if let E::StructField(t) = &item.inner {
        ty(t)
    } else {
        "_".into()
    }
}

/// Maps rustdoc's item enum to the index kind; `None` for items the index leaves out.
fn kind_of(inner: &E, assoc: bool) -> Option<ItemKind> {
    use ItemKind as K;
    Some(match inner {
        E::Module(_) => K::Module,
        E::Struct(_) => K::Struct,
        E::Union(_) => K::Union,
        E::Enum(_) => K::Enum,
        E::Variant(_) => K::Variant,
        E::StructField(_) => K::Field,
        E::Function(_) if assoc => K::Method,
        E::Function(_) => K::Function,
        E::Trait(_) => K::Trait,
        E::TraitAlias(_) => K::TraitAlias,
        E::TypeAlias(_) => K::TypeAlias,
        E::Constant { .. } => K::Constant,
        E::Static(_) => K::Static,
        E::AssocConst { .. } => K::AssocConst,
        E::AssocType { .. } => K::AssocType,
        E::Macro(_) => K::Macro,
        E::ProcMacro(_) => K::ProcMacro,
        E::ExternType => K::ExternType,
        E::Use(_) | E::Impl(_) | E::ExternCrate { .. } | E::Primitive(_) => return None,
    })
}

/// The first paragraph of a doc comment, with its lines joined by single spaces.
fn first_paragraph(docs: &str) -> String {
    let lines = docs.lines().map(str::trim).skip_while(|l| l.is_empty());
    let para: Vec<&str> = lines.take_while(|l| !l.is_empty()).collect();
    para.join(" ")
}

/// `#[doc(alias = "a")]` and `#[doc(alias("a", "b"))]` values, in attribute order. Aliases
/// cannot contain `"` (rustdoc rejects it), so splitting on quotes is exact.
fn aliases(attrs: &[Attribute]) -> Vec<String> {
    let texts = attrs.iter().filter_map(|a| match a {
        Attribute::Other(text) if text.replace(' ', "").starts_with("#[doc(") => {
            text.find("alias").map(|start| &text[start..])
        }
        _ => None,
    });
    texts
        .flat_map(|t| t.split('"').skip(1).step_by(2).map(str::to_owned))
        .collect()
}
