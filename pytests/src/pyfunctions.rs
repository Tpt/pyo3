use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

#[pyfunction(signature = ())]
fn none() {}

type Any<'py> = Bound<'py, PyAny>;
type Dict<'py> = Bound<'py, PyDict>;
type Tuple<'py> = Bound<'py, PyTuple>;

#[pyfunction(signature = (a, b = None, *, c = None))]
fn simple<'py>(
    a: Any<'py>,
    b: Option<Any<'py>>,
    c: Option<Any<'py>>,
) -> (Any<'py>, Option<Any<'py>>, Option<Any<'py>>) {
    (a, b, c)
}

#[pyfunction(signature = (a, b = None, *args, c = None))]
fn simple_args<'py>(
    a: Any<'py>,
    b: Option<Any<'py>>,
    args: Tuple<'py>,
    c: Option<Any<'py>>,
) -> (Any<'py>, Option<Any<'py>>, Tuple<'py>, Option<Any<'py>>) {
    (a, b, args, c)
}

#[pyfunction(signature = (a, b = None, c = None, **kwargs))]
fn simple_kwargs<'py>(
    a: Any<'py>,
    b: Option<Any<'py>>,
    c: Option<Any<'py>>,
    kwargs: Option<Dict<'py>>,
) -> (
    Any<'py>,
    Option<Any<'py>>,
    Option<Any<'py>>,
    Option<Dict<'py>>,
) {
    (a, b, c, kwargs)
}

#[pyfunction(signature = (a, b = None, *args, c = None, **kwargs))]
fn simple_args_kwargs<'py>(
    a: Any<'py>,
    b: Option<Any<'py>>,
    args: Tuple<'py>,
    c: Option<Any<'py>>,
    kwargs: Option<Dict<'py>>,
) -> (
    Any<'py>,
    Option<Any<'py>>,
    Tuple<'py>,
    Option<Any<'py>>,
    Option<Dict<'py>>,
) {
    (a, b, args, c, kwargs)
}

#[pyfunction(signature = (*args, **kwargs))]
fn args_kwargs<'py>(
    args: Tuple<'py>,
    kwargs: Option<Dict<'py>>,
) -> (Tuple<'py>, Option<Dict<'py>>) {
    (args, kwargs)
}

#[pyfunction(signature = (a, /, b))]
fn positional_only<'py>(a: Any<'py>, b: Any<'py>) -> (Any<'py>, Any<'py>) {
    (a, b)
}

#[pymodule]
pub mod pyfunctions {
    #[pymodule_export]
    use super::{
        args_kwargs, none, positional_only, simple, simple_args, simple_args_kwargs, simple_kwargs,
    };
}
