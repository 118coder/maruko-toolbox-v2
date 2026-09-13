

mod commands;







use commands::{make_emitter, AppState};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, Theme,
};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            // 状态（设置加载 + 工具链定位 + 任务队列）
            let emitter = make_emitter(handle.clone());
            let state = AppState::new(emitter);
            let splash_enabled = state.settings.lock().unwrap().splash;
            let dark = state.settings.lock().unwrap().dark;
            let tray_mode = state.settings.lock().unwrap().tray_mode;
            app.manage(state);
            maruko_core::logs::write(&format!("小丸工具箱V2现代版 {} 启动", commands::VERSION));

            // 主题
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_theme(Some(if dark { Theme::Dark } else { Theme::Light }));
                if !splash_enabled {
                    let _ = w.show();
                    if let Some(s) = app.get_webview_window("splash") {
                        let _ = s.close();
                    }
                }
            }

            // 托盘
            let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let icon = app.default_window_icon().cloned();
            if let Some(icon) = icon {
                let mut builder = TrayIconBuilder::with_id("maruko-tray")
                    .icon(icon)
                    .tooltip("小丸工具箱V2现代版")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| {
                        match event.id().as_ref() {
                            "show" => {
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                            "quit" => {
                                app.exit(0);
                            }
                            _ => {}
                        }
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                            let app = tray.app_handle();
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    });
                let tray = builder.build(app)?;
                let _ = tray.set_visible(tray_mode);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // 托盘模式：点关闭 = 隐藏到托盘
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let hide = window
                    .app_handle()
                    .try_state::<AppState>()
                    .map(|s| s.settings.lock().unwrap().tray_mode)
                    .unwrap_or(false);
                if hide {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::ui_ready,
            commands::log_frontend,
            commands::get_settings,
            commands::save_settings,
            commands::reset_settings,
            commands::get_tools_info,
            commands::check_ffmpeg_features,
            commands::pick_file,
            commands::pick_folder,
            commands::file_exists,
            commands::open_path,
            commands::open_url,
            commands::mediainfo_analyze,
            commands::encode_video,
            commands::encode_video_batch,
            commands::encode_audio,
            commands::encode_audio_batch,
            commands::merge_audio,
            commands::mux_mp4,
            commands::mux_mkv,
            commands::convert_mux,
            commands::extract_common,
            commands::extract_flv,
            commands::identify_mkv,
            commands::extract_mkv_track,
            commands::one_pic,
            commands::cut_video,
            commands::rotate_video,
            commands::save_avs,
            commands::avs_preview,
            commands::job_cancel,
            commands::shutdown_cancel_cmd,
            commands::get_logs,
            commands::open_logs_dir,
            commands::delete_logs,
            commands::check_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
