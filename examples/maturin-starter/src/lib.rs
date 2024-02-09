use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::wrap_pymodule;

mod submodule;

#[pyclass]
struct ExampleClass {
    #[pyo3(get, set)]
    value: i32,
}

#[pymethods]
impl ExampleClass {
    #[new]
    pub fn new(value: i32) -> Self {
        ExampleClass { value }
    }
}

/// An example module implemented in Rust using PyO3.
#[pymodule]
mod maturin_starter {
    #[pyo3]
    use super::{submodule::submodule, ExampleClass};

    #[pymodule_init]
    fn init(m: &PyModule) -> PyResult<()> {
        let sys = PyModule::import(m.py(), "sys")?;
        let sys_modules: &PyDict = sys.getattr("modules")?.downcast()?;
        sys_modules.set_item("maturin_starter.submodule", m.getattr("submodule")?)
    }
}
