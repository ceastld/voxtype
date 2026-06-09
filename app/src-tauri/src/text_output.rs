use enigo::{Enigo, Keyboard, Settings};

/// Type Unicode text into the focused window via simulated keyboard input.
pub fn type_unicode(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo.text(text).map_err(|e| e.to_string())?;
    Ok(())
}
