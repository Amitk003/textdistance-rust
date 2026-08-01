use pyo3::prelude::*;

/// Extension module `textdistance._textdistance`.
///
/// Kernel functions are registered here as they are ported. The Python
/// adapter in `python/textdistance/` calls into this module and never
/// reimplements algorithm math.
#[pymodule]
fn _textdistance(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
