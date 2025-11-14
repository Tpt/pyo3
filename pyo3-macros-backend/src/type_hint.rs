//! Define a small DSL to write Python type hint using Rust syntax.
//!
//! It is used by the `#[pyo3(signature =)]` attribute

use crate::utils::PyO3CratePath;
use proc_macro2::{Ident, TokenStream};
use quote::{quote, ToTokens};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Result, Token};

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
}

// Utility types

/// Root of the AST: the union of type hints like A | B
#[derive(Clone, Debug, PartialEq, Eq)]
struct UnionTypeHint(Punctuated<PythonIdentifier, Token![|]>);

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
    pub fn to_introspection_token_stream(&self, pyo3_crate_path: &PyO3CratePath) -> TokenStream {
        let elts = self
            .0
            .iter()
            .map(|elt| elt.to_introspection_token_stream(pyo3_crate_path));
        quote! { #pyo3_crate_path::inspect::TypeHint::union(&[#(#elts),*]) }
    }
}

/// Leaf of the ast: a path like typing.Any
#[derive(Clone, Debug, PartialEq, Eq)]
struct PythonIdentifier(Punctuated<Ident, Token![.]>);

impl Parse for PythonIdentifier {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        Ok(Self(Punctuated::parse_separated_nonempty(input)?))
    }
}

impl ToTokens for PythonIdentifier {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens)
    }
}

impl PythonIdentifier {
    pub fn to_introspection_token_stream(&self, pyo3_crate_path: &PyO3CratePath) -> TokenStream {
        // We need guess what is the module and what is the local name
        // The module is every ident before the last one that do not start with an uppercase letter
        let mut module_part = Vec::new();
        let mut name_part = Vec::new();
        let is_module = true;
        for item in &self.0 {
            let item = item.unraw().to_string();
            if is_module && item.chars().next().is_some_and(|c| c.is_lowercase()) {
                module_part.push(item);
            } else {
                name_part.push(item);
            }
        }
        if name_part.is_empty() {
            // The last element is always the name
            name_part.push(module_part.pop().expect("We have always at least one item"));
        }
        let module = module_part.join(".");
        let name = name_part.join(".");
        if module.is_empty() {
            quote! { #pyo3_crate_path::inspect::TypeHint::local(#name) }
        } else {
            quote! { #pyo3_crate_path::inspect::TypeHint::module_attr(#module, #name) }
        }
    }
}
