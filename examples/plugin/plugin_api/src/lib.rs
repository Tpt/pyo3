use pyo3::prelude::*;

///this is our Gadget that python plugin code can create, and rust app can then access natively.
#[pyclass]
pub struct Gadget {
    #[pyo3(get, set)]
    pub prop: usize,
    //this field will only be accessible to rust code
    pub rustonly: Vec<usize>,
}

#[pymethods]
impl Gadget {
    #[new]
    fn new() -> Self {
        Gadget {
            prop: 777,
            rustonly: Vec::new(),
        }
    }

    fn push(&mut self, v: usize) {
        self.rustonly.push(v);
    }
}

/// A Python module for plugin interface types
#[pymodule]
pub mod plugin_api {
    #[pyo3]
    use super::Gadget;
}
