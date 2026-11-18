use pyo3::{exceptions::PyRuntimeError, prelude::*};
use raad::sdk;
fn error(value: raad::Error) -> PyErr {
    PyRuntimeError::new_err(sdk::error_json(value))
}

#[pyclass(frozen, name = "Repository")]
struct Repository {
    inner: sdk::Repository,
}
#[pymethods]
impl Repository {
    #[new]
    fn new(py: Python<'_>, options: String) -> PyResult<Self> {
        py.detach(|| sdk::Repository::open(&options))
            .map(|inner| Self { inner })
            .map_err(error)
    }
    #[pyo3(signature = (request, token=None))]
    fn call(&self, py: Python<'_>, request: String, token: Option<String>) -> PyResult<String> {
        py.detach(|| self.inner.call(&request, token))
            .map_err(error)
    }
}
#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Repository>()?;
    Ok(())
}
