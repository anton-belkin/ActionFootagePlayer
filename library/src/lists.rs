// The lists the player, gyroflow-export and the Library share (mpv/scripts/workspace/registry.json, afp_catalog::registry's
// types): embedded here, parsed once; the page gets the same text as window.AFP_LISTS before its scripts run (main.rs)
use afp_catalog::registry::Registry;
use std::sync::OnceLock;

pub const TEXT: &str = include_str!("../../mpv/scripts/workspace/registry.json");

pub fn get() -> &'static Registry {
    static REG: OnceLock<Registry> = OnceLock::new();
    REG.get_or_init(|| afp_catalog::registry::parse(TEXT))
}

/// What the page's scripts find as window.AFP_LISTS (library/ui-tests/page.js sets it the same way)
pub fn page_script() -> String { format!("window.AFP_LISTS = {TEXT};") }
