use crate::model::{Class, Function, Module, ParameterKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Generates the [type stubs](https://typing.readthedocs.io/en/latest/source/stubs.html) of a given module.
/// It returns a map between the file name and the file content.
/// The root module stubs will be in the `__init__.pyi` file and the submodules directory
/// in files with a relevant name.
pub fn module_stub_files(module: &Module) -> HashMap<PathBuf, String> {
    let mut output_files = HashMap::new();
    add_module_stub_files(module, Path::new(""), &mut output_files);
    output_files
}

fn add_module_stub_files(
    module: &Module,
    module_path: &Path,
    output_files: &mut HashMap<PathBuf, String>,
) {
    output_files.insert(module_path.join("__init__.pyi"), module_stubs(module));
    for submodule in &module.modules {
        if submodule.modules.is_empty() {
            output_files.insert(
                module_path.join(format!("{}.pyi", submodule.name)),
                module_stubs(submodule),
            );
        } else {
            add_module_stub_files(submodule, &module_path.join(&submodule.name), output_files);
        }
    }
}

/// Generates the module stubs to a String, not including submodules
fn module_stubs(module: &Module) -> String {
    let mut elements = Vec::new();
    for class in &module.classes {
        elements.push(class_stubs(class));
    }
    for function in &module.functions {
        elements.push(function_stubs(function));
    }
    elements.push(String::new()); // last line jump
    elements.join("\n")
}

fn class_stubs(class: &Class) -> String {
    format!("class {}: ...", class.name)
}

fn function_stubs(function: &Function) -> String {
    // Signature
    let mut positional_only = true;
    let mut keyword_only = false;
    let mut parameters = Vec::new();
    for parameter in &function.signature.parameters {
        if positional_only && !matches!(parameter.kind, ParameterKind::PositionalOnly) {
            if !parameters.is_empty() {
                parameters.push("/".into());
            }
            positional_only = false;
        }
        if !keyword_only && matches!(parameter.kind, ParameterKind::KeywordOnly) {
            parameters.push("*".into());
            keyword_only = true;
        }
        parameters.push(match parameter.kind {
            ParameterKind::VarPositional => {
                keyword_only = true;
                format!("*{}", parameter.name)
            }
            ParameterKind::VarKeyword => format!("**{}", parameter.name),
            _ => {
                let mut s = parameter.name.clone();
                if let Some(annotation) = &parameter.annotation {
                    s.push_str(": ");
                    s.push_str(annotation);
                }
                if parameter.has_default {
                    s.push_str(" = ...");
                }
                s
            }
        });
    }

    format!("def {}({}): ...", function.name, parameters.join(", "))
}
