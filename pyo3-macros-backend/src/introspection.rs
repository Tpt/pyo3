//! Generates introspection data i.e. JSON strings in the .pyo3i0 section.
//!
//! There is a JSON per PyO3 proc macro (pyclass, pymodule, pyfunction...).
//!
//! These JSON blobs can refer to each others via the _PYO3_INTROSPECTION_ID constants
//! providing unique ids for each element.

use crate::method::{FnArg, RegularArg};
use crate::pyfunction::FunctionSignature;
use crate::utils::PyO3CratePath;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem::take;
use std::sync::atomic::{AtomicUsize, Ordering};
use syn::{
    AngleBracketedGenericArguments, GenericArgument, Ident, Lifetime, Path, PathArguments,
    PathSegment, Type, TypeArray, TypePath, TypeReference,
};

static GLOBAL_COUNTER_FOR_UNIQUE_NAMES: AtomicUsize = AtomicUsize::new(0);

pub fn module_introspection_code<'a>(
    pyo3_crate_path: &PyO3CratePath,
    name: &str,
    members: impl IntoIterator<Item = &'a Ident>,
) -> TokenStream {
    IntrospectionNode::Map(
        [
            ("type", IntrospectionNode::String("module")),
            ("id", IntrospectionNode::IntrospectionId(None)),
            ("name", IntrospectionNode::String(name)),
            (
                "members",
                IntrospectionNode::List(
                    members
                        .into_iter()
                        .map(|member| IntrospectionNode::IntrospectionId(Some(member)))
                        .collect(),
                ),
            ),
        ]
        .into(),
    )
    .emit(pyo3_crate_path)
}

pub fn class_introspection_code(
    pyo3_crate_path: &PyO3CratePath,
    ident: &Ident,
    name: &str,
) -> TokenStream {
    IntrospectionNode::Map(
        [
            ("type", IntrospectionNode::String("class")),
            ("id", IntrospectionNode::IntrospectionId(Some(ident))),
            ("name", IntrospectionNode::String(name)),
        ]
        .into(),
    )
    .emit(pyo3_crate_path)
}

pub fn function_introspection_code(
    pyo3_crate_path: &PyO3CratePath,
    ident: &Ident,
    name: &str,
    signature: &FunctionSignature<'_>,
) -> TokenStream {
    IntrospectionNode::Map(
        [
            ("type", IntrospectionNode::String("function")),
            ("id", IntrospectionNode::IntrospectionId(Some(ident))),
            ("name", IntrospectionNode::String(name)),
            (
                "signature",
                signature_introspection_data(pyo3_crate_path, signature),
            ),
        ]
        .into(),
    )
    .emit(pyo3_crate_path)
}

fn signature_introspection_data<'a>(
    pyo3_crate_path: &PyO3CratePath,
    signature: &'a FunctionSignature<'a>,
) -> IntrospectionNode<'a> {
    let mut argument_desc = signature.arguments.iter().filter_map(|arg| {
        if let FnArg::Regular(arg) = arg {
            Some(arg)
        } else {
            None
        }
    });
    let mut parameters = Vec::new();
    for (i, param) in signature
        .python_signature
        .positional_parameters
        .iter()
        .enumerate()
    {
        let Some(arg_desc) = argument_desc.next() else {
            panic!("Less arguments than in python signature");
        };
        parameters.push(parameter_introspection_data(
            pyo3_crate_path,
            param,
            if i < signature.python_signature.positional_only_parameters {
                "POSITIONAL_ONLY"
            } else {
                "POSITIONAL_OR_KEYWORD"
            },
            arg_desc,
        ));
    }
    if let Some(param) = &signature.python_signature.varargs {
        parameters.push(IntrospectionNode::Map(
            [
                ("name", IntrospectionNode::String(param)),
                ("kind", IntrospectionNode::String("VAR_POSITIONAL")),
            ]
            .into(),
        ));
    }
    for (param, _) in &signature.python_signature.keyword_only_parameters {
        let Some(arg_desc) = argument_desc.next() else {
            panic!("Less arguments than in python signature");
        };
        parameters.push(parameter_introspection_data(
            pyo3_crate_path,
            param,
            "KEYWORD_ONLY",
            arg_desc,
        ));
    }
    if let Some(param) = &signature.python_signature.kwargs {
        parameters.push(IntrospectionNode::Map(
            [
                ("name", IntrospectionNode::String(param)),
                ("kind", IntrospectionNode::String("VAR_KEYWORD")),
            ]
            .into(),
        ));
    }
    IntrospectionNode::Map([("parameters", IntrospectionNode::List(parameters))].into())
}

fn parameter_introspection_data<'a>(
    pyo3_crate_path: &PyO3CratePath,
    name: &'a str,
    kind: &'a str,
    desc: &'a RegularArg<'_>,
) -> IntrospectionNode<'a> {
    let mut params: HashMap<_, _> = [
        ("name", IntrospectionNode::String(name)),
        ("kind", IntrospectionNode::String(kind)),
        (
            "has_default",
            IntrospectionNode::Bool(desc.default_value.is_some()),
        ),
    ]
    .into();
    if desc.from_py_with.is_none() {
        if let Some(ty) = desc.option_wrapped_type {
            let ty = remove_bound_lifetimes(ty.clone());
            params.insert(
                "annotation",
                IntrospectionNode::ToStringExpression(
                    quote! {
                        #pyo3_crate_path::impl_::concat::const_concat!(
                            <#ty as #pyo3_crate_path::impl_::extract_argument::PyFunctionArgument>::INPUT_TYPE,
                            " | None"
                        )
                    },
                ),
            );
        } else {
            let ty = remove_bound_lifetimes(desc.ty.clone());
            params.insert(
                "annotation",
                IntrospectionNode::ToStringExpression(
                    quote! { <#ty as #pyo3_crate_path::impl_::extract_argument::PyFunctionArgument>::INPUT_TYPE },
                ),
            );
        }
    }
    IntrospectionNode::Map(params)
}

enum IntrospectionNode<'a> {
    Bool(bool),
    String(&'a str),
    IntrospectionId(Option<&'a Ident>),
    ToStringExpression(TokenStream),
    Map(HashMap<&'static str, IntrospectionNode<'a>>),
    List(Vec<IntrospectionNode<'a>>),
}

impl IntrospectionNode<'_> {
    fn emit(self, pyo3_crate_path: &PyO3CratePath) -> TokenStream {
        let mut content = ConcatenationBuilder::default();
        self.add_to_serialization(&mut content);
        let content = content.into_token_stream(pyo3_crate_path);

        let static_name = format_ident!("PYO3_INTROSPECTION_0_{}", unique_element_id());
        // #[no_mangle] is required to make sure some linkers like Linux ones do not mangle the section name too.
        quote! {
            const _: () = {
                #[used]
                #[no_mangle]
                static #static_name: &'static str = #content;
            };
        }
    }

    fn add_to_serialization(self, content: &mut ConcatenationBuilder) {
        match self {
            Self::Bool(value) => content.push_str(if value { "true" } else { "false" }),
            Self::String(string) => {
                content.push_str_to_escape(string);
            }
            Self::IntrospectionId(ident) => {
                content.push_str("\"");
                content.push_token(if let Some(ident) = ident {
                    quote! { #ident::_PYO3_INTROSPECTION_ID}
                } else {
                    quote! { _PYO3_INTROSPECTION_ID }
                });
                content.push_str("\"");
            }
            Self::ToStringExpression(expr) => {
                content.push_str("\"");
                content.push_token(expr);
                content.push_str("\"");
            }
            Self::Map(map) => {
                content.push_str("{");
                for (i, (key, value)) in map.into_iter().enumerate() {
                    if i > 0 {
                        content.push_str(",");
                    }
                    content.push_str_to_escape(key);
                    content.push_str(":");
                    value.add_to_serialization(content);
                }
                content.push_str("}");
            }
            Self::List(list) => {
                content.push_str("[");
                for (i, value) in list.into_iter().enumerate() {
                    if i > 0 {
                        content.push_str(",");
                    }
                    value.add_to_serialization(content);
                }
                content.push_str("]");
            }
        }
    }
}

#[derive(Default)]
struct ConcatenationBuilder {
    elements: Vec<TokenStream>,
    current_string: String,
}

impl ConcatenationBuilder {
    fn push_token(&mut self, token: TokenStream) {
        if !self.current_string.is_empty() {
            let str = take(&mut self.current_string);
            self.elements.push(quote! { #str });
        }
        self.elements.push(token);
    }

    fn push_str(&mut self, value: &str) {
        self.current_string.push_str(value);
    }

    fn push_str_to_escape(&mut self, value: &str) {
        self.current_string.push('"');
        for c in value.chars() {
            match c {
                '\\' => self.current_string.push_str("\\\\"),
                '"' => self.current_string.push_str("\\\""),
                c => {
                    if c < char::from(32) {
                        panic!("ASCII chars below 32 are not allowed")
                    } else {
                        self.current_string.push(c);
                    }
                }
            }
        }
        self.current_string.push('"');
    }

    fn into_token_stream(self, pyo3_crate_path: &PyO3CratePath) -> TokenStream {
        let mut elements = self.elements;
        if !self.current_string.is_empty() {
            let str = self.current_string;
            elements.push(quote! { #str });
        }

        quote! {
            #pyo3_crate_path::impl_::concat::const_concat!(#(#elements , )*)
        }
    }
}

pub fn introspection_id_const() -> TokenStream {
    let id = unique_element_id().to_string();
    quote! {
        #[doc(hidden)]
        pub const _PYO3_INTROSPECTION_ID: &'static str = #id;
    }
}

fn unique_element_id() -> u64 {
    let mut hasher = DefaultHasher::new();
    format!("{:?}", Span::call_site()).hash(&mut hasher); // Distinguishes between call sites
    GLOBAL_COUNTER_FOR_UNIQUE_NAMES
        .fetch_add(1, Ordering::Relaxed)
        .hash(&mut hasher); // If there are multiple elements in the same call site
    hasher.finish()
}

fn remove_bound_lifetimes(t: Type) -> Type {
    // Type::Path { qself: None, path: Path { leading_colon: None, segments: [PathSegment { ident: Ident { ident: "Any", span: #0 bytes(24699..24702) }, arguments: PathArguments::AngleBracketed { colon2_token: None, lt_token: Lt, args: [GenericArgument::Lifetime(Lifetime { apostrophe: #0 bytes(24703..24706), ident: Ident { ident: "py", span: #0 bytes(24703..24706) } })], gt_token: Gt } }] } }
    match t {
        Type::Array(t) => Type::Array(TypeArray {
            bracket_token: t.bracket_token,
            elem: Box::new(remove_bound_lifetimes(*t.elem)),
            semi_token: t.semi_token,
            len: t.len,
        }),
        Type::Path(t) => Type::Path(TypePath {
            qself: t.qself,
            path: Path {
                leading_colon: t.path.leading_colon,
                segments: t
                    .path
                    .segments
                    .into_iter()
                    .map(|s| PathSegment {
                        ident: s.ident,
                        arguments: match s.arguments {
                            PathArguments::AngleBracketed(a) => {
                                PathArguments::AngleBracketed(AngleBracketedGenericArguments {
                                    colon2_token: a.colon2_token,
                                    lt_token: a.lt_token,
                                    args: a
                                        .args
                                        .into_iter()
                                        .map(|a| match a {
                                            GenericArgument::Lifetime(l) => {
                                                GenericArgument::Lifetime(Lifetime::new(
                                                    "'_",
                                                    l.span(),
                                                ))
                                            }
                                            _ => a,
                                        })
                                        .collect(),
                                    gt_token: a.gt_token,
                                })
                            }
                            a => a,
                        },
                    })
                    .collect(),
            },
        }),
        Type::Reference(t) => Type::Reference(TypeReference {
            and_token: t.and_token,
            lifetime: None,
            mutability: t.mutability,
            elem: Box::new(remove_bound_lifetimes(*t.elem)),
        }),
        t => {
            println!("{t:?}");
            t
        }
    }
}
