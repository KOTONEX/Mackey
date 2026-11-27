// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Mackey contributors

interface 窗口接口 {
    get_wm_class(): string | null;
    get_gtk_application_id(): string | null;
    get_sandboxed_app_id(): string | null;
    get_wm_class_instance(): string | null;
    get_title(): string | null;
    is_skip_taskbar(): boolean;
}

interface 窗口演员接口 {
    meta_window: 窗口接口 | null;
}

declare var global: {
    display: {
        focus_window: 窗口接口 | null;
    };
    get_window_actors(): 窗口演员接口[];
};
