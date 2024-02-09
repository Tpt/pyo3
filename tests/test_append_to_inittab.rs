#![cfg(all(feature = "macros", not(PyPy)))]
use pyo3::prelude::*;

#[pyfunction]
fn foo() -> usize {
    123
}

#[pymodule]
mod module_with_functions {
    #[pyo3]
    use super::foo;
}

#[cfg(not(PyPy))]
#[test]
fn test_module_append_to_inittab() {
    use pyo3::append_to_inittab;
    append_to_inittab!(module_with_functions);
    Python::with_gil(|py| {
        py.run(
            r#"
import module_with_functions
assert module_with_functions.foo() == 123
"#,
            None,
            None,
        )
        .map_err(|e| e.display(py))
        .unwrap();
    })
}
