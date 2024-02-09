use pyo3::prelude::*;
use pyo3::types::PyString;

#[pyfunction]
fn vec_to_vec_i32(vec: Vec<i32>) -> Vec<i32> {
    vec
}

#[pyfunction]
fn array_to_array_i32(arr: [i32; 3]) -> [i32; 3] {
    arr
}

#[pyfunction]
fn vec_to_vec_pystring(vec: Vec<&PyString>) -> Vec<&PyString> {
    vec
}

#[pymodule]
pub mod sequence {
    #[pyo3]
    use super::{array_to_array_i32, vec_to_vec_i32, vec_to_vec_pystring};
}
