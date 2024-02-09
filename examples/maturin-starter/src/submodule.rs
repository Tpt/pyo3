use pyo3::prelude::*;

#[pyclass]
struct SubmoduleClass {}

#[pymethods]
impl SubmoduleClass {
    #[new]
    pub fn __new__() -> Self {
        SubmoduleClass {}
    }

    pub fn greeting(&self) -> &'static str {
        "Hello, world!"
    }
}

#[pymodule]
pub mod submodule {
    #[pyo3]
    use super::SubmoduleClass;
}
