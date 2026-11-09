// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Mackey contributors
//
// 本文件只补充 @girs 官方声明未覆盖的 GNOME Shell 全局对象。
// GJS/Gio/GLib 与扩展基类类型来自随仓库分发的 @girs 声明（见 类型声明/girs/说明.md），
// 由 Cargo 构建时的原生 tsc 与编辑器共同消费。

/** MetaWindow 上本仓库用到的只读接口（部分窗口类型上这些方法会抛错，调用方自己防御）。 */
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

/** GNOME Shell 在扩展进程中提供的全局对象。 */
declare var global: {
    display: {
        focus_window: 窗口接口 | null;
    };
    get_window_actors(): 窗口演员接口[];
};
