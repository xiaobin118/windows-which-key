use anyhow::Result;
use serde_json::json;
use windows::Win32::Foundation::HWND;
use crate::types::UiCommand;

pub const FRONTEND_HTML: &str = include_str!("frontend.html");

// Stub implementation for now
// Full WebView2 integration requires async initialization with callbacks
pub struct WebView2Bridge {
    _hwnd: HWND,
}

impl WebView2Bridge {
    pub fn new(hwnd: HWND) -> Result<Self> {
        // TODO: Implement proper WebView2 initialization with async callbacks
        log::info!("WebView2 bridge initialized (stub mode)");
        Ok(WebView2Bridge { _hwnd: hwnd })
    }

    pub fn load_html(&self, _html: &str) -> Result<()> {
        // TODO: Implement HTML loading via WebView2
        log::debug!("Load HTML (stub)");
        Ok(())
    }

    pub fn send_command(&self, cmd: &UiCommand) -> Result<()> {
        // TODO: Implement JSON message sending via WebView2
        let json_msg = match cmd {
            UiCommand::Show { entries, breadcrumb, .. } => {
                json!({
                    "type": "show",
                    "entries": entries,
                    "breadcrumb": breadcrumb
                })
            }
            UiCommand::UpdateEntries { entries, breadcrumb } => {
                json!({
                    "type": "update",
                    "entries": entries,
                    "breadcrumb": breadcrumb
                })
            }
            UiCommand::Hide => {
                json!({
                    "type": "hide"
                })
            }
        };

        log::debug!("Send command (stub): {}", json_msg);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn test_json_serialization() {
        let cmd = UiCommand::Show {
            position: (100, 100),
            entries: vec![
                DisplayEntry {
                    key: "C-c".to_string(),
                    desc: "Copy".to_string(),
                    is_group: false,
                }
            ],
            breadcrumb: vec![],
        };

        let json_str = match &cmd {
            UiCommand::Show { position: _, entries, breadcrumb } => {
                json!({
                    "type": "show",
                    "entries": entries,
                    "breadcrumb": breadcrumb
                }).to_string()
            }
            _ => unreachable!(),
        };

        assert!(json_str.contains("show"));
        assert!(json_str.contains("Copy"));
    }
}
