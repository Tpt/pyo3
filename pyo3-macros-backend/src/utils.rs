use crate::attributes::{CrateAttribute, RenamingRule};
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned, ToTokens};
use std::ffi::CString;
use std::mem::take;
use syn::spanned::Spanned;
use syn::{Expr, ExprLit, Lit};

/// Macro inspired by `anyhow::anyhow!` to create a compiler error with the given span.
macro_rules! err_spanned {
    ($span:expr => $msg:expr) => {
        syn::Error::new($span, $msg)
    };
}

/// Macro inspired by `anyhow::bail!` to return a compiler error with the given span.
macro_rules! bail_spanned {
    ($span:expr => $msg:expr) => {
        return Err(err_spanned!($span => $msg))
    };
}

/// Macro inspired by `anyhow::ensure!` to return a compiler error with the given span if the
/// specified condition is not met.
macro_rules! ensure_spanned {
    ($condition:expr, $span:expr => $msg:expr) => {
        if !($condition) {
            bail_spanned!($span => $msg);
        }
    };
    ($($condition:expr, $span:expr => $msg:expr;)*) => {
        if let Some(e) = [$(
            (!($condition)).then(|| err_spanned!($span => $msg)),
        )*]
            .into_iter()
            .flatten()
            .reduce(|mut acc, e| {
                acc.combine(e);
                acc
            }) {
                return Err(e);
            }
    };
}

/// Check if the given type `ty` is `pyo3::Python`.
pub fn is_python(ty: &syn::Type) -> bool {
    match unwrap_ty_group(ty) {
        syn::Type::Path(typath) => typath
            .path
            .segments
            .last()
            .map(|seg| seg.ident == "Python")
            .unwrap_or(false),
        _ => false,
    }
}

/// If `ty` is `Option<T>`, return `Some(T)`, else `None`.
pub fn option_type_argument(ty: &syn::Type) -> Option<&syn::Type> {
    if let syn::Type::Path(syn::TypePath { path, .. }) = ty {
        let seg = path.segments.last().filter(|s| s.ident == "Option")?;
        if let syn::PathArguments::AngleBracketed(params) = &seg.arguments {
            if let syn::GenericArgument::Type(ty) = params.args.first()? {
                return Some(ty);
            }
        }
    }
    None
}

// TODO: Replace usage of this by [`syn::LitCStr`] when on MSRV 1.77
#[derive(Clone)]
pub struct LitCStr {
    lit: CString,
    span: Span,
    pyo3_path: PyO3CratePath,
}

impl LitCStr {
    pub fn new(lit: CString, span: Span, ctx: &Ctx) -> Self {
        Self {
            lit,
            span,
            pyo3_path: ctx.pyo3_path.clone(),
        }
    }

    pub fn empty(ctx: &Ctx) -> Self {
        Self {
            lit: CString::new("").unwrap(),
            span: Span::call_site(),
            pyo3_path: ctx.pyo3_path.clone(),
        }
    }
}

impl ToTokens for LitCStr {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if cfg!(c_str_lit) {
            syn::LitCStr::new(&self.lit, self.span).to_tokens(tokens);
        } else {
            let pyo3_path = &self.pyo3_path;
            let lit = self.lit.to_str().unwrap();
            tokens.extend(quote_spanned!(self.span => #pyo3_path::ffi::c_str!(#lit)));
        }
    }
}

/// A syntax tree which evaluates to a nul-terminated docstring for Python.
///
/// Typically the tokens will just be that string, but if the original docs included macro
/// expressions then the tokens will be a concat!("...", "\n", "\0") expression of the strings and
/// macro parts. contents such as parse the string contents.
#[derive(Clone)]
pub struct PythonDoc {
    parts: Vec<StrOrExpr>,
}

/// A plain string or an expression
#[derive(Clone)]
enum StrOrExpr {
    Str(String),
    Expr(Expr),
}

/// Collects all #[doc = "..."] attributes into a TokenStream evaluating to a null-terminated string.
///
/// If this doc is for a callable, the provided `text_signature` can be passed to prepend
/// this to the documentation suitable for Python to extract this into the `__text_signature__`
/// attribute.
pub fn get_doc(attrs: &[syn::Attribute], mut text_signature: Option<String>) -> PythonDoc {
    // insert special divider between `__text_signature__` and doc
    // (assume text_signature is itself well-formed)
    if let Some(text_signature) = &mut text_signature {
        text_signature.push_str("\n--\n\n");
    }

    let mut parts = Vec::new();
    let mut first = true;
    let mut current_part = text_signature.unwrap_or_default();
    let mut current_part_span = None;

    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Ok(nv) = attr.meta.require_name_value() {
                current_part_span = match current_part_span {
                    None => Some(nv.value.span()),
                    Some(span) => span.join(nv.value.span()),
                };
                if !first {
                    current_part.push('\n');
                } else {
                    first = false;
                }
                if let Expr::Lit(ExprLit {
                    lit: Lit::Str(lit_str),
                    ..
                }) = &nv.value
                {
                    // Strip single left space from literal strings, if needed.
                    // e.g. `/// Hello world` expands to #[doc = "Hello world"]
                    let doc_line = lit_str.value();
                    current_part.push_str(doc_line.strip_prefix(' ').unwrap_or(&doc_line));
                } else {
                    // This is probably a macro doc from Rust 1.54, e.g. #[doc = include_str!(...)]
                    // Reset the string buffer, write that part, and then push this macro part too.
                    parts.push(StrOrExpr::Str(take(&mut current_part)));
                    current_part.clear();
                    parts.push(StrOrExpr::Expr(nv.value.clone()));
                }
            }
        }
    }
    // Doc contained macro pieces - return as `concat!` expression
    if !current_part.is_empty() {
        parts.push(StrOrExpr::Str(current_part));
    }
    PythonDoc { parts }
}

impl PythonDoc {
    pub fn to_cstr_stream(&self, ctx: &Ctx) -> TokenStream {
        let parts = &self.parts;
        if let [StrOrExpr::Str(value)] = &parts[..] {
            // Simple case, a single stream. We append a null bytes to get a valid C string
            if let Ok(null_terminated_value) = CString::new(value.clone()) {
                return null_terminated_value.into_token_stream();
            } // We ignore the error here,  c_str! will report it
        }
        let Ctx { pyo3_path, .. } = ctx;
        quote!(#pyo3_path::ffi::c_str!(concat!(#(#parts),*)))
    }
}

impl ToTokens for StrOrExpr {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Str(s) => s.to_tokens(tokens),
            Self::Expr(e) => e.to_tokens(tokens),
        }
    }
}

pub fn unwrap_ty_group(mut ty: &syn::Type) -> &syn::Type {
    while let syn::Type::Group(g) = ty {
        ty = &*g.elem;
    }
    ty
}

pub struct Ctx {
    /// Where we can find the pyo3 crate
    pub pyo3_path: PyO3CratePath,

    /// If we are in a pymethod or pyfunction,
    /// this will be the span of the return type
    pub output_span: Span,
}

impl Ctx {
    pub(crate) fn new(attr: &Option<CrateAttribute>, signature: Option<&syn::Signature>) -> Self {
        let pyo3_path = match attr {
            Some(attr) => PyO3CratePath::Given(attr.value.0.clone()),
            None => PyO3CratePath::Default,
        };

        let output_span = if let Some(syn::Signature {
            output: syn::ReturnType::Type(_, output_type),
            ..
        }) = &signature
        {
            output_type.span()
        } else {
            Span::call_site()
        };

        Self {
            pyo3_path,
            output_span,
        }
    }
}

#[derive(Clone)]
pub enum PyO3CratePath {
    Given(syn::Path),
    Default,
}

impl PyO3CratePath {
    pub fn to_tokens_spanned(&self, span: Span) -> TokenStream {
        match self {
            Self::Given(path) => quote_spanned! { span => #path },
            Self::Default => quote_spanned! {  span => ::pyo3 },
        }
    }
}

impl ToTokens for PyO3CratePath {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Given(path) => path.to_tokens(tokens),
            Self::Default => quote! { ::pyo3 }.to_tokens(tokens),
        }
    }
}

pub fn apply_renaming_rule(rule: RenamingRule, name: &str) -> String {
    use heck::*;

    match rule {
        RenamingRule::CamelCase => name.to_lower_camel_case(),
        RenamingRule::KebabCase => name.to_kebab_case(),
        RenamingRule::Lowercase => name.to_lowercase(),
        RenamingRule::PascalCase => name.to_upper_camel_case(),
        RenamingRule::ScreamingKebabCase => name.to_shouty_kebab_case(),
        RenamingRule::ScreamingSnakeCase => name.to_shouty_snake_case(),
        RenamingRule::SnakeCase => name.to_snake_case(),
        RenamingRule::Uppercase => name.to_uppercase(),
    }
}

pub(crate) enum IdentOrStr<'a> {
    Str(&'a str),
    Ident(syn::Ident),
}

pub(crate) fn has_attribute(attrs: &[syn::Attribute], ident: &str) -> bool {
    has_attribute_with_namespace(attrs, None, &[ident])
}

pub(crate) fn has_attribute_with_namespace(
    attrs: &[syn::Attribute],
    crate_path: Option<&PyO3CratePath>,
    idents: &[&str],
) -> bool {
    let mut segments = vec![];
    if let Some(c) = crate_path {
        match c {
            PyO3CratePath::Given(paths) => {
                for p in &paths.segments {
                    segments.push(IdentOrStr::Ident(p.ident.clone()));
                }
            }
            PyO3CratePath::Default => segments.push(IdentOrStr::Str("pyo3")),
        }
    };
    for i in idents {
        segments.push(IdentOrStr::Str(i));
    }

    attrs.iter().any(|attr| {
        segments
            .iter()
            .eq(attr.path().segments.iter().map(|v| &v.ident))
    })
}

pub fn expr_to_python(expr: &syn::Expr) -> String {
    match expr {
        // literal values
        syn::Expr::Lit(syn::ExprLit { lit, .. }) => match lit {
            syn::Lit::Str(s) => s.token().to_string(),
            syn::Lit::Char(c) => c.token().to_string(),
            syn::Lit::Int(i) => i.base10_digits().to_string(),
            syn::Lit::Float(f) => f.base10_digits().to_string(),
            syn::Lit::Bool(b) => {
                if b.value() {
                    "True".to_string()
                } else {
                    "False".to_string()
                }
            }
            _ => "...".to_string(),
        },
        // None
        syn::Expr::Path(syn::ExprPath { qself, path, .. })
            if qself.is_none() && path.is_ident("None") =>
        {
            "None".to_string()
        }
        // others, unsupported yet so defaults to `...`
        _ => "...".to_string(),
    }
}
