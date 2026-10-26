//! Renders rustdoc JSON types, bounds and generics back into one-line Rust syntax.

use rustdoc_types::{Abi, AssocItemConstraintKind as Bind, FunctionHeader, FunctionSignature};
use rustdoc_types::{GenericArg, GenericArgs, GenericBound as B, GenericParamDef};
use rustdoc_types::{GenericParamDefKind as P, Generics, Path, PreciseCapturingArg, Term};
use rustdoc_types::{TraitBoundModifier as M, Type as T, WherePredicate as W};

/// `pre + s + post`, or nothing when `s` is empty.
pub(crate) fn wrap(pre: &str, s: String, post: &str) -> String {
    if s.is_empty() {
        s
    } else {
        format!("{pre}{s}{post}")
    }
}

/// Renders a type.
pub(crate) fn ty(t: &T) -> String {
    match t {
        T::ResolvedPath(p) => path(p),
        T::DynTrait(d) => {
            let all: Vec<String> = (d.traits.iter())
                .map(|p| hrtb(&p.generic_params) + &path(&p.trait_))
                .chain(d.lifetime.clone())
                .collect();
            format!("dyn {}", all.join(" + "))
        }
        T::Generic(name) | T::Primitive(name) => name.clone(),
        T::Array { type_, len } => format!("[{}; {len}]", ty(type_)),
        T::Pat { type_, .. } => ty(type_),
        T::ImplTrait(b) => format!("impl {}", bounds(b)),
        T::Infer => "_".to_owned(),
        T::RawPointer {
            is_mutable: m,
            type_,
        } => {
            format!("*{} {}", if *m { "mut" } else { "const" }, ty(type_))
        }
        T::BorrowedRef {
            lifetime: l,
            is_mutable: m,
            type_,
        } => {
            format!("&{}{}", lt_mut(l, *m), ty(type_))
        }
        T::QualifiedPath {
            name,
            args,
            self_type: s,
            trait_,
        } => {
            let args = args.as_deref().map(generic_args).unwrap_or_default();
            match trait_ {
                Some(tr) if !tr.path.is_empty() => {
                    format!("<{} as {}>::{name}{args}", ty(s), path(tr))
                }
                _ => format!("{}::{name}{args}", ty(s)),
            }
        }
        T::Tuple(items) if items.len() == 1 => format!("({},)", ty(&items[0])),
        T::Tuple(items) => format!("({})", join(items.iter().map(ty))),
        T::Slice(inner) => format!("[{}]", ty(inner)),
        T::FunctionPointer(fp) => {
            let (h, sig) = (header(&fp.header), sig_tail(&fp.sig, true));
            format!("{}{h}fn{sig}", hrtb(&fp.generic_params))
        }
    }
}

/// `'a mut ` / `mut ` / `'a ` / nothing, for references and `self` receivers.
fn lt_mut(lifetime: &Option<String>, is_mutable: bool) -> String {
    let lt = wrap("", lifetime.clone().unwrap_or_default(), " ");
    format!("{lt}{}", if is_mutable { "mut " } else { "" })
}

fn path(p: &Path) -> String {
    p.path.clone() + &p.args.as_deref().map(generic_args).unwrap_or_default()
}

fn generic_args(a: &GenericArgs) -> String {
    match a {
        GenericArgs::AngleBracketed { args, constraints } => {
            let args = args.iter().map(|g| match g {
                GenericArg::Lifetime(l) => l.clone(),
                GenericArg::Type(t) => ty(t),
                GenericArg::Const(c) => c.expr.clone(),
                GenericArg::Infer => "_".to_owned(),
            });
            let constraints = constraints.iter().map(|c| {
                let args = c.args.as_deref().map(generic_args).unwrap_or_default();
                let name = c.name.clone() + &args;
                match &c.binding {
                    Bind::Equality(t) => format!("{name} = {}", term(t)),
                    Bind::Constraint(b) => format!("{name}: {}", bounds(b)),
                }
            });
            wrap("<", join(args.chain(constraints)), ">")
        }
        GenericArgs::Parenthesized { inputs, output } => {
            let out = wrap(" -> ", output.as_ref().map(ty).unwrap_or_default(), "");
            format!("({}){out}", join(inputs.iter().map(ty)))
        }
        GenericArgs::ReturnTypeNotation => "(..)".to_owned(),
    }
}

fn term(t: &Term) -> String {
    match t {
        Term::Type(t) => ty(t),
        Term::Constant(c) => c.expr.clone(),
    }
}

/// Renders bounds joined with ` + `.
pub(crate) fn bounds(bounds: &[B]) -> String {
    let one = |b: &B| match b {
        B::TraitBound {
            trait_,
            generic_params: g,
            modifier,
        } => {
            let m = match modifier {
                M::None => "",
                M::Maybe => "?",
                M::MaybeConst => "[const] ",
            };
            format!("{}{m}{}", hrtb(g), path(trait_))
        }
        B::Outlives(l) => l.clone(),
        B::Use(args) => {
            let arg = |a: &PreciseCapturingArg| match a {
                PreciseCapturingArg::Lifetime(s) | PreciseCapturingArg::Param(s) => s.clone(),
            };
            format!("use<{}>", join(args.iter().map(arg)))
        }
    };
    bounds.iter().map(one).collect::<Vec<_>>().join(" + ")
}

fn hrtb(params: &[GenericParamDef]) -> String {
    wrap("for<", join(params.iter().map(param)), "> ")
}

fn param(p: &GenericParamDef) -> String {
    let name = &p.name;
    match &p.kind {
        P::Lifetime { outlives } => format!("{name}{}", wrap(": ", outlives.join(" + "), "")),
        P::Type {
            bounds: b, default, ..
        } => {
            let default = wrap(" = ", default.as_ref().map(ty).unwrap_or_default(), "");
            format!("{name}{}{default}", wrap(": ", bounds(b), ""))
        }
        P::Const { type_, default } => {
            let default = wrap(" = ", default.clone().unwrap_or_default(), "");
            format!("const {name}: {}{default}", ty(type_))
        }
    }
}

/// Renders `<params>`, leaving out the synthetic parameters rustdoc adds for `impl Trait` arguments.
pub(crate) fn generic_params(g: &Generics) -> String {
    let synthetic = |p: &&GenericParamDef| {
        matches!(
            p.kind,
            P::Type {
                is_synthetic: true,
                ..
            }
        )
    };
    wrap(
        "<",
        join(g.params.iter().filter(|p| !synthetic(p)).map(param)),
        ">",
    )
}

/// Renders ` where ...`, or nothing.
pub(crate) fn where_clause(g: &Generics) -> String {
    let pred = |w: &W| match w {
        W::BoundPredicate {
            type_,
            bounds: b,
            generic_params: g,
        } => {
            format!("{}{}: {}", hrtb(g), ty(type_), bounds(b))
        }
        W::LifetimePredicate { lifetime, outlives } => {
            format!("{lifetime}: {}", outlives.join(" + "))
        }
        W::EqPredicate { lhs, rhs } => format!("{} == {}", ty(lhs), term(rhs)),
    };
    wrap(" where ", join(g.where_predicates.iter().map(pred)), "")
}

/// Renders the qualifiers before `fn`: `const async unsafe extern "C" `.
pub(crate) fn header(h: &FunctionHeader) -> String {
    let abi = match &h.abi {
        Abi::Rust => "",
        Abi::C { .. } => "C",
        Abi::Cdecl { .. } => "cdecl",
        Abi::Stdcall { .. } => "stdcall",
        Abi::Fastcall { .. } => "fastcall",
        Abi::Aapcs { .. } => "aapcs",
        Abi::Win64 { .. } => "win64",
        Abi::SysV64 { .. } => "sysv64",
        Abi::System { .. } => "system",
        Abi::Other(o) => o,
    };
    let flags = [
        (h.is_const, "const "),
        (h.is_async, "async "),
        (h.is_unsafe, "unsafe "),
    ];
    let flags: String = flags.iter().filter(|f| f.0).map(|f| f.1).collect();
    flags + &wrap("extern \"", abi.to_owned(), "\" ")
}

/// Renders `(args) -> ret`. `self` receivers are shortened to `&self`, `&mut self` or `self`.
pub(crate) fn sig_tail(sig: &FunctionSignature, pointer: bool) -> String {
    let is_self = |t: &T| matches!(t, T::Generic(g) if g == "Self");
    let arg = |(name, t): &(String, T)| match (name.as_str(), t) {
        ("self", t) if is_self(t) => "self".to_owned(),
        (
            "self",
            T::BorrowedRef {
                lifetime: l,
                is_mutable: m,
                type_,
            },
        ) if is_self(type_) => {
            format!("&{}self", lt_mut(l, *m))
        }
        ("_", t) if pointer => ty(t),
        (n, t) => format!("{n}: {}", ty(t)),
    };
    let variadic = sig.is_c_variadic.then(|| "...".to_owned());
    let ret = wrap(" -> ", sig.output.as_ref().map(ty).unwrap_or_default(), "");
    format!(
        "({}){ret}",
        join(sig.inputs.iter().map(arg).chain(variadic))
    )
}

fn join(items: impl Iterator<Item = String>) -> String {
    items.collect::<Vec<_>>().join(", ")
}
