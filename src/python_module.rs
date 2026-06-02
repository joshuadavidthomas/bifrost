use crate::{
    SearchToolsService, SearchToolsServiceError, SearchToolsServiceErrorCode,
    searchtools_render::RenderOptions,
};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use std::path::PathBuf;

#[pyclass(name = "SearchToolsNativeSession")]
pub struct SearchToolsNativeSession {
    inner: SearchToolsService,
}

#[pymethods]
impl SearchToolsNativeSession {
    #[new]
    fn new(py: Python<'_>, root: &str) -> PyResult<Self> {
        let root = PathBuf::from(root);
        let service = py
            .allow_threads(|| SearchToolsService::new_for_python(root))
            .map_err(PyRuntimeError::new_err)?;
        Ok(Self { inner: service })
    }

    fn call_tool_json(&self, py: Python<'_>, name: &str, arguments_json: &str) -> PyResult<String> {
        let name = name.to_owned();
        let arguments_json = arguments_json.to_owned();
        let result = py.allow_threads(|| self.inner.call_tool_json(&name, &arguments_json));

        match result {
            Ok(payload) => Ok(payload),
            Err(err) => Err(service_error_to_py(err)),
        }
    }

    fn call_tool_payload_json(
        &self,
        py: Python<'_>,
        name: &str,
        arguments_json: &str,
        render_line_numbers: bool,
    ) -> PyResult<String> {
        let name = name.to_owned();
        let arguments_json = arguments_json.to_owned();
        let result = py.allow_threads(|| {
            self.inner.call_tool_payload_json(
                &name,
                &arguments_json,
                RenderOptions {
                    render_line_numbers,
                },
            )
        });

        match result {
            Ok(payload) => Ok(payload),
            Err(err) => Err(service_error_to_py(err)),
        }
    }

    fn close(&self) -> PyResult<()> {
        self.inner.close().map_err(service_error_to_py)
    }
}

fn service_error_to_py(err: SearchToolsServiceError) -> PyErr {
    match err.code {
        SearchToolsServiceErrorCode::InvalidParams => PyValueError::new_err(err.message),
        SearchToolsServiceErrorCode::UnknownTool | SearchToolsServiceErrorCode::Internal => {
            PyRuntimeError::new_err(err.message)
        }
    }
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<SearchToolsNativeSession>()?;
    Ok(())
}
