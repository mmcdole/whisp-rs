use anyhow::Result;

use crate::uinput::VirtualKeyboard;

pub fn emit_text(text: &str, vkbd: &mut VirtualKeyboard) -> Result<()> {
    vkbd.type_text(text)?;
    log::info!("Output: typed {} chars via uinput", text.len());
    Ok(())
}
