mod commands;
mod neuroskill;
mod provider;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());

    // Global shortcut is desktop-only.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_global_shortcut::Builder::new().build());
    }

    builder
        .setup(|app| {
            // Custom unified toolbar (Titlebar.svelte) draws the window chrome.
            // macOS keeps its native traffic lights via the Overlay title-bar
            // style (set in tauri.conf.json); every other platform draws its own
            // min/maximize/close caption buttons, so turn the OS frame off there.
            #[cfg(not(target_os = "macos"))]
            {
                use tauri::Manager;
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.set_decorations(false);
                }
            }

            // Register a system-wide hotkey that surfaces the window and opens
            // quick-capture. On some Linux window managers global shortcuts may
            // be intercepted by the WM — that's a known limitation, not a bug.
            //
            // TODO(quick-capture): a polished version would open a dedicated,
            // borderless "spotlight" window instead of focusing the main one.
            #[cfg(desktop)]
            {
                use tauri::{Emitter, Manager};
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };

                let shortcut =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
                let handle = app.handle().clone();
                if let Err(e) =
                    app.global_shortcut()
                        .on_shortcut(shortcut, move |_app, _sc, event| {
                            if event.state() == ShortcutState::Pressed {
                                if let Some(win) = handle.get_webview_window("main") {
                                    let _ = win.show();
                                    let _ = win.unminimize();
                                    let _ = win.set_focus();
                                    let _ = win.emit("waid://quick-capture", ());
                                }
                            }
                        })
                {
                    eprintln!("WAID: could not register global shortcut: {e}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_briefs,
            commands::read_brief,
            commands::save_brief,
            commands::touch_brief,
            commands::set_brief_status,
            commands::append_capture,
            commands::fire_webhook,
            commands::save_brief_webhook,
            commands::delete_brief_webhook,
            commands::sync_brief,
            commands::sync_all,
            commands::synthesize_brief,
            commands::synthesize_all,
            commands::list_ollama_models,
            commands::get_llm_settings,
            commands::set_llm_settings,
            commands::get_briefs_dir,
            commands::set_briefs_dir,
            commands::get_vault_info,
            commands::is_wsl,
            commands::create_brief,
            commands::set_secret,
            commands::get_secret,
            commands::delete_secret,
            commands::has_secret,
            commands::save_brief_connection,
            commands::delete_brief_connection,
            commands::save_brief_integration,
            commands::delete_brief_integration,
            commands::test_brief_connection,
            commands::connect_gmail,
            commands::fetch_integration,
            commands::sync_mind_state,
            commands::mark_brief_session,
            commands::digest_integrations,
            commands::morning_briefing,
            commands::generate_gmail_query,
            commands::generate_slack_query,
            commands::bootstrap_from_folder,
            commands::bootstrap_from_github,
            commands::bootstrap_from_answers,
            commands::normalize_bootstrap_paste,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
