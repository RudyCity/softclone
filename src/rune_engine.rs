//! Rune only receives display-name data. It has no filesystem or process bindings.
use rune::{Context, Source, Sources};

const LABEL_SCRIPT: &str = include_str!("../scripts/label.rn");

pub fn clone_label(base: &str, index: i64) -> Result<String, String> {
    let context = Context::with_default_modules().map_err(|e| e.to_string())?;
    let mut sources = Sources::new();
    sources.insert(Source::memory(LABEL_SCRIPT).map_err(|e| e.to_string())?);
    let mut vm = rune::prepare(&mut sources)
        .with_context(&context)
        .build_vm()
        .map_err(|e| e.to_string())?;
    let output = vm
        .call(["clone_label"], (base.to_string(), index))
        .map_err(|e| e.to_string())?;
    let label: String = rune::from_value(output).map_err(|e| e.to_string())?;
    crate::core::validate_name(&label).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rune_suggests_a_clone_name() {
        assert_eq!(clone_label("Notepad", 2).unwrap(), "Notepad copy 2");
    }
}
