fn main() {
    // Expose the build target triple so main.rs can resolve the sidecar path at runtime.
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rustc-env=TARGET_TRIPLE={target}");

    // Tauri 2's ACL only allows commands that have a permission referencing
    // them. For user-defined commands (#[tauri::command]) the build helper can
    // autogenerate `allow-$command` / `deny-$command` permissions if we list
    // every command here. Without this, invoke() rejects user commands with
    // "X not allowed. Plugin not found." Keep this list in sync with the
    // invoke_handler!{} list in src/main.rs.
    let attrs =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "retry_daemon_connect",
            "open_diagnostics",
            "quit_app",
            "restart_app",
            "view_crash_report",
            "restart_daemon_anyway",
            "open_vault_window",
            "set_window_title",
            "confirm_window_close",
            "open_folder_as_vault",
            "list_open_vaults",
            "close_vault_window",
            "pick_vault_folder",
            "agent_list",
            "agent_config_get",
            "agent_config_set",
            "agent_start",
            "agent_prompt",
            "agent_select_model",
            "agent_set_read_only",
            "agent_answer_permission",
            "agent_stop",
            "agent_diagnostics",
        ]));
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}
