use anyhow::{Context, Result};
use serde_json::json;
use windows::core::Interface;
use windows::Win32::Foundation::{E_POINTER, HWND, RECT};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    CreateCoreWebView2EnvironmentWithOptions,
    ICoreWebView2,
    ICoreWebView2Controller,
    ICoreWebView2Controller2,
    COREWEBVIEW2_COLOR,
};
use webview2_com::{
    CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
};
use crate::types::UiCommand;

pub const FRONTEND_HTML: &str = include_str!("frontend.html");

pub struct WebView2Bridge {
    _controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
}

impl WebView2Bridge {
    /// 初始化 WebView2（环境 + controller）。异步 COM 操作会阻塞等待完成。
    pub fn new(hwnd: HWND) -> Result<Self> {
        unsafe {
            // 初始化 COM（APARTMENTTHREADED）
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            // ── 创建环境（异步回调，阻塞等待） ──
            let environment = {
                let (tx, rx) = std::sync::mpsc::channel();

                CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
                    Box::new(move |handler| {
                        CreateCoreWebView2EnvironmentWithOptions(None, None, None, &handler)
                            .map_err(webview2_com::Error::WindowsError)
                    }),
                    Box::new(move |result, environment| {
                        result?;
                        tx.send(
                            environment.ok_or_else(|| {
                                windows::core::Error::from(E_POINTER)
                            }),
                        )
                        .expect("发送环境结果失败");
                        Ok(())
                    }),
                )
                .map_err(|e| anyhow::anyhow!("创建 WebView2 环境失败: {:?}", e))?;

                rx.recv()
                    .map_err(|_| anyhow::anyhow!("等待 WebView2 环境超时"))??
            };

            // ── 创建 controller（异步回调，阻塞等待） ──
            let controller = {
                let (tx, rx) = std::sync::mpsc::channel();

                CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
                    Box::new(move |handler| {
                        environment
                            .CreateCoreWebView2Controller(hwnd, &handler)
                            .map_err(webview2_com::Error::WindowsError)
                    }),
                    Box::new(move |result, controller| {
                        result?;
                        tx.send(
                            controller.ok_or_else(|| {
                                windows::core::Error::from(E_POINTER)
                            }),
                        )
                        .expect("发送 controller 结果失败");
                        Ok(())
                    }),
                )
                .map_err(|e| anyhow::anyhow!("创建 WebView2 controller 失败: {:?}", e))?;

                rx.recv()
                    .map_err(|_| anyhow::anyhow!("等待 WebView2 controller 超时"))??
            };

            // ── 设置 controller 尺寸和可见性 ──
            controller.SetBounds(RECT {
                left: 0,
                top: 0,
                right: 400,
                bottom: 300,
            })
            .context("设置 WebView2 bounds 失败")?;
            controller
                .SetIsVisible(true)
                .context("设置 WebView2 可见性失败")?;

            // The HTML background can only be translucent if the WebView2
            // controller itself also has an alpha-zero background.
            let controller2: ICoreWebView2Controller2 = controller
                .cast()
                .context("获取 WebView2 controller2 失败")?;
            controller2
                .SetDefaultBackgroundColor(COREWEBVIEW2_COLOR { A: 0, R: 0, G: 0, B: 0 })
                .context("设置 WebView2 透明背景失败")?;

            let webview = controller
                .CoreWebView2()
                .context("获取 CoreWebView2 失败")?;

            // ── 配置 WebView ──
            let settings = webview.Settings().context("获取 WebView2 settings 失败")?;
            settings.SetAreDefaultContextMenusEnabled(false)?;
            settings.SetIsStatusBarEnabled(false)?;

            log::info!("WebView2 初始化成功");

            Ok(Self {
                _controller: controller,
                webview,
            })
        }
    }

    pub fn load_html(&self, html: &str) -> Result<()> {
        unsafe {
            let html_str: windows::core::HSTRING = html.into();
            self.webview
                .NavigateToString(&html_str)
                .context("加载 HTML 失败")?;
        }
        Ok(())
    }

    pub fn set_bounds(&self, width: i32, height: i32) -> Result<()> {
        unsafe {
            self._controller
                .SetBounds(RECT { left: 0, top: 0, right: width, bottom: height })
                .context("更新 WebView2 bounds 失败")?;
        }
        Ok(())
    }

    /// 向前端推送 UI 命令（JSON 序列化后通过 postMessage 注入 JS）
    pub fn send_command(&self, cmd: &UiCommand) -> Result<()> {
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

        let js_code = format!("window.postMessage({}, '*');", json_msg.to_string());
        let js_str: windows::core::HSTRING = js_code.into();

        unsafe {
            self.webview
                .ExecuteScript(&js_str, None)
                .context("向前端推送命令失败")?;
        }
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
            entries: vec![DisplayEntry {
                key: "C-c".to_string(),
                desc: "Copy".to_string(),
                is_group: false,
            }],
            breadcrumb: vec![],
        };

        let json_str = match &cmd {
            UiCommand::Show { entries, breadcrumb, .. } => {
                json!({
                    "type": "show",
                    "entries": entries,
                    "breadcrumb": breadcrumb
                })
                .to_string()
            }
            _ => unreachable!(),
        };

        assert!(json_str.contains("show"));
        assert!(json_str.contains("Copy"));
    }
}
