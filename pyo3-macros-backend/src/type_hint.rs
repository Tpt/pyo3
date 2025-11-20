//! Define a small DSL to write Python type hint using Rust syntax.
//!
//! It is used by the `#[pyo3(signature =)]` attribute

use crate::utils::PyO3CratePath;
use proc_macro2::{Ident, TokenStream};
use quote::{quote, ToTokens};
use std::borrow::Cow;
use std::iter::once;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::Bracket;
use syn::visit_mut::{visit_type_mut, VisitMut};
use syn::{bracketed, Lifetime, Result, Token, Type};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PythonTypeHint(UnionTypeHint);

impl Parse for PythonTypeHint {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        Ok(Self(input.parse()?))
    }
}

impl ToTokens for PythonTypeHint {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens)
    }
}

impl PythonTypeHint {
    pub fn to_introspection_token_stream(&self, pyo3_crate_path: &PyO3CratePath) -> TokenStream {
        self.0.to_introspection_token_stream(pyo3_crate_path)
    }

    /// Build the union of the different element
    pub fn union<T: Into<Self>>(elements: impl IntoIterator<Item = T>) -> Self {
        Self(UnionTypeHint(
            elements.into_iter().flat_map(|e| e.into().0 .0).collect(),
        ))
    }

    /// Build the subscripted type value[slice[0], ..., slice[n]]
    pub fn subscript<T: Into<Self>>(
        value: PythonIdentifier,
        slice: impl IntoIterator<Item = T>,
    ) -> Self {
        Self(UnionTypeHint(
            once(MaybeSubscriptedTypeHint {
                value,
                subscript: Some(SubscriptTypeHint {
                    bracket: Bracket::default(),
                    slice: slice.into_iter().map(Into::into).collect(),
                }),
            })
            .collect(),
        ))
    }
}

impl From<PythonIdentifier> for PythonTypeHint {
    fn from(id: PythonIdentifier) -> Self {
        Self(UnionTypeHint(
            once(MaybeSubscriptedTypeHint {
                value: id,
                subscript: None,
            })
            .collect(),
        ))
    }
}

/// Root of the AST: the union of type hints like A | B
#[derive(Clone, Debug, PartialEq, Eq)]
struct UnionTypeHint(Punctuated<MaybeSubscriptedTypeHint, Token![|]>);

impl Parse for UnionTypeHint {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        Ok(Self(Punctuated::parse_separated_nonempty(input)?))
    }
}

impl ToTokens for UnionTypeHint {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens)
    }
}

impl UnionTypeHint {
    fn to_introspection_token_stream(&self, pyo3_crate_path: &PyO3CratePath) -> TokenStream {
        if self.0.len() == 1 {
            // Single element, we just serialize it
            return self
                .0
                .first()
                .unwrap()
                .to_introspection_token_stream(pyo3_crate_path);
        }
        let elts = self
            .0
            .iter()
            .map(|elt| elt.to_introspection_token_stream(pyo3_crate_path));
        quote! { #pyo3_crate_path::inspect::TypeHint::union(&[#(#elts),*]) }
    }
}

/// A ma element value[slice[0], ..., slice[n]]
#[derive(Clone, Debug, PartialEq, Eq)]
struct MaybeSubscriptedTypeHint {
    value: PythonIdentifier,
    subscript: Option<SubscriptTypeHint>,
}

impl Parse for MaybeSubscriptedTypeHint {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        Ok(Self {
            value: input.parse()?,
            subscript: if input.lookahead1().peek(Bracket) {
                Some(input.parse()?)
            } else {
                None
            },
        })
    }
}

impl ToTokens for MaybeSubscriptedTypeHint {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.value.to_tokens(tokens);
        self.subscript.to_tokens(tokens);
    }
}

impl MaybeSubscriptedTypeHint {
    fn to_introspection_token_stream(&self, pyo3_crate_path: &PyO3CratePath) -> TokenStream {
        let value = self.value.to_introspection_token_stream(pyo3_crate_path);
        if let Some(subscript) = &self.subscript {
            let slice = subscript
                .slice
                .iter()
                .map(|elt| elt.to_introspection_token_stream(pyo3_crate_path));
            quote! { #pyo3_crate_path::inspect::TypeHint::subscript(&#value, &[#(#slice),*]) }
        } else {
            value
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SubscriptTypeHint {
    bracket: Bracket,
    slice: Punctuated<PythonTypeHint, Token![,]>,
}

impl Parse for SubscriptTypeHint {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let slice;
        Ok(Self {
            bracket: bracketed!(slice in input),
            slice: slice.parse_terminated(PythonTypeHint::parse, Token![,])?,
        })
    }
}

impl ToTokens for SubscriptTypeHint {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.bracket
            .surround(tokens, |tokens| self.slice.to_tokens(tokens))
    }
}

/// Leaf of the ast: a path like typing.Any
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PythonIdentifier(PythonIdentifierVariant);

#[derive(Clone, Debug, PartialEq, Eq)]
enum PythonIdentifierVariant {
    /// A python identifier written as a Rust AST based on Ident
    /// Used in explicit type hint in signatures
    Rust {
        module: Punctuated<Ident, Token![.]>,
        name: Punctuated<Ident, Token![.]>,
    },
    /// The Python type hint of a FromPyObject implementation
    RustFromPyObjectType(Type),
    /// The Python type hint of a IntoPyObject implementation
    RustIntoPyObjectType(Type),
    /// The Python type matching the given Rust type given as a function argument
    RustArgumentType(Type),
    /// The Python type matching the given Rust type given as a function returned value
    RustReturnType(Type),
    /// A literal Python type. Used by the macros
    Python {
        module: Cow<'static, str>, // empty module = local element
        name: Cow<'static, str>,
    },
}

impl Parse for PythonIdentifier {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut module_part = Vec::new();
        let mut name_part = Vec::new();
        let is_module = true;
        // We split the path into module part and local part
        // We assume the module part is always lower case
        for item in Punctuated::<Ident, Token![.]>::parse_separated_nonempty(input)? {
            if is_module
                && item
                    .unraw()
                    .to_string()
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_lowercase())
            {
                module_part.push(item);
            } else {
                name_part.push(item);
            }
        }
        if name_part.is_empty() {
            // The last element is always the name
            name_part.push(module_part.pop().expect("We have always at least one item"));
        }
        Ok(Self(PythonIdentifierVariant::Rust {
            module: module_part.into_iter().collect(),
            name: name_part.into_iter().collect(),
        }))
    }
}

impl ToTokens for PythonIdentifier {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match &self.0 {
            PythonIdentifierVariant::Rust { module, name } => {
                if !module.is_empty() {
                    module.to_tokens(tokens);
                    <Token![.]>::default().to_tokens(tokens);
                }
                name.to_tokens(tokens);
            }
            _ => {
                // ToTokens is only used in signature errors, where the other variants are unused
                unimplemented!()
            }
        }
    }
}

impl PythonIdentifier {
    /// Build from a local name
    pub fn local(name: impl Into<Cow<'static, str>>) -> Self {
        Self::module_attr("", name)
    }

    /// Build from a builtins name like `None`
    pub fn builtin(name: impl Into<Cow<'static, str>>) -> Self {
        Self::module_attr("builtins", name)
    }

    /// Build from a module and a name like `collections.abc` and `Sequence`
    pub fn module_attr(
        module: impl Into<Cow<'static, str>>,
        name: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self(PythonIdentifierVariant::Python {
            module: module.into(),
            name: name.into(),
        })
    }

    /// The type hint of a FromPyObject implementation as a function argument
    ///
    /// If self_type is set, Self in the given type will be replaced by self_type
    pub fn from_from_py_object(t: Type, self_type: Option<&Type>) -> Self {
        Self(PythonIdentifierVariant::RustFromPyObjectType(clean_type(
            t, self_type,
        )))
    }

    /// The type hint of a IntoPyObject implementation as a function argument
    ///
    /// If self_type is set, Self in the given type will be replaced by self_type
    pub fn from_into_py_object(t: Type, self_type: Option<&Type>) -> Self {
        Self(PythonIdentifierVariant::RustIntoPyObjectType(clean_type(
            t, self_type,
        )))
    }

    /// The type hint of the Rust type used as a function argument
    ///
    /// If self_type is set, Self in the given type will be replaced by self_type
    pub fn from_argument_type(t: Type, self_type: Option<&Type>) -> Self {
        Self(PythonIdentifierVariant::RustArgumentType(clean_type(
            t, self_type,
        )))
    }

    /// The type hint of the Rust type used as a function output type
    ///
    /// If self_type is set, Self in the given type will be replaced by self_type
    pub fn from_return_type(t: Type, self_type: Option<&Type>) -> Self {
        Self(PythonIdentifierVariant::RustReturnType(clean_type(
            t, self_type,
        )))
    }

    fn to_introspection_token_stream(&self, pyo3_crate_path: &PyO3CratePath) -> TokenStream {
        match &self.0 {
            PythonIdentifierVariant::Rust { module, name } => {
                Self::introspection_token_stream_from_module_and_name(
                    &module
                        .iter()
                        .map(|n| n.unraw().to_string())
                        .collect::<Vec<_>>()
                        .join("."),
                    &name
                        .iter()
                        .map(|n| n.unraw().to_string())
                        .collect::<Vec<_>>()
                        .join("."),
                    pyo3_crate_path,
                )
            }
            PythonIdentifierVariant::Python { module, name } => {
                Self::introspection_token_stream_from_module_and_name(module, name, pyo3_crate_path)
            }
            PythonIdentifierVariant::RustFromPyObjectType(t) => {
                quote! { <#t as #pyo3_crate_path::FromPyObject<'_, '_>>::INPUT_TYPE }
            }
            PythonIdentifierVariant::RustIntoPyObjectType(t) => {
                quote! { <#t as #pyo3_crate_path::IntoPyObject<'_>>::OUTPUT_TYPE }
            }
            PythonIdentifierVariant::RustArgumentType(t) => {
                quote! {
                    <#t as #pyo3_crate_path::impl_::extract_argument::PyFunctionArgument<
                        {
                            #[allow(unused_imports, reason = "`Probe` trait used on negative case only")]
                            use #pyo3_crate_path::impl_::pyclass::Probe as _;
                            #pyo3_crate_path::impl_::pyclass::IsFromPyObject::<#t>::VALUE
                        }
                    >>::INPUT_TYPE
                }
            }
            PythonIdentifierVariant::RustReturnType(t) => {
                quote! { <#t as #pyo3_crate_path::impl_::introspection::PyReturnType>::OUTPUT_TYPE }
            }
        }
    }

    fn introspection_token_stream_from_module_and_name(
        module: &str,
        name: &str,
        pyo3_crate_path: &PyO3CratePath,
    ) -> TokenStream {
        if module.is_empty() {
            quote! { #pyo3_crate_path::inspect::TypeHint::local(#name) }
        } else if module == "builtins" {
            quote! { #pyo3_crate_path::inspect::TypeHint::builtin(#name) }
        } else {
            quote! { #pyo3_crate_path::inspect::TypeHint::module_attr(#module, #name) }
        }
    }
}

fn clean_type(mut t: Type, self_type: Option<&Type>) -> Type {
    if let Some(self_type) = self_type {
        replace_self(&mut t, self_type);
    }
    elide_lifetimes(&mut t);
    t
}

/// Replaces all explicit lifetimes in `self` with elided (`'_`) lifetimes
///
/// This is useful if `Self` is used in `const` context, where explicit
/// lifetimes are not allowed (yet).
fn elide_lifetimes(ty: &mut Type) {
    struct ElideLifetimesVisitor;

    impl VisitMut for ElideLifetimesVisitor {
        fn visit_lifetime_mut(&mut self, l: &mut syn::Lifetime) {
            *l = Lifetime::new("'_", l.span());
        }
    }

    ElideLifetimesVisitor.visit_type_mut(ty);
}

// Replace Self in types with the given type
fn replace_self(ty: &mut Type, self_target: &Type) {
    struct SelfReplacementVisitor<'a> {
        self_target: &'a Type,
    }

    impl VisitMut for SelfReplacementVisitor<'_> {
        fn visit_type_mut(&mut self, ty: &mut Type) {
            if let Type::Path(type_path) = ty {
                if type_path.qself.is_none()
                    && type_path.path.segments.len() == 1
                    && type_path.path.segments[0].ident == "Self"
                    && type_path.path.segments[0].arguments.is_empty()
                {
                    // It is Self
                    *ty = self.self_target.clone();
                    return;
                }
            }
            visit_type_mut(self, ty);
        }
    }

    SelfReplacementVisitor { self_target }.visit_type_mut(ty);
}
