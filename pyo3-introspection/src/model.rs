#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct Module {
    pub name: String,
    pub modules: Vec<Module>,
    pub classes: Vec<Class>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct Class {
    pub name: String,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct Function {
    pub name: String,
    pub signature: Signature,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct Signature {
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct Parameter {
    pub name: String,
    pub kind: ParameterKind,
    pub has_default: bool,
    pub annotation: Option<String>,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]
pub enum ParameterKind {
    PositionalOnly,
    PositionalOrKeyword,
    VarPositional,
    KeywordOnly,
    VarKeyword,
}
