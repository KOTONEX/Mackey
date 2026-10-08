// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Mackey contributors
//
// 接口契约源自 xremap-gnome（https://github.com/xremap/xremap-gnome,
// Copyright (C) 2022 Takashi Kokubun, GPLv2+）；本文件是对该公开 D-Bus 接口的独立实现。
//
// xremap 焦点桥 —— 只做一件事：把「当前焦点窗口的 wm_class / title」通过 D-Bus 暴露出去。
//
// 背景：xremap 在 GNOME Wayland 上做 application 匹配时，会调用
//   org.gnome.Shell:/com/k0kubun/Xremap -> com.k0kubun.Xremap.ActiveWindow() -> s(JSON)
// 上游扩展 xremap@k0kubun.com 在 GNOME 45+ 上已失效（xremap 启动时会打印
// `application-client: GNOME (supported: false)`），导致「终端/应用差异化映射」全部失效。
//
// 本扩展用**同一份 D-Bus 契约**重新实现它，因此无需给 xremap 打补丁。
// 安全边界：仅读取焦点窗口元数据；不抓键、不注入按键、不写任何文件。

import Gio from 'gi://Gio';
import type { DBusExportedObject } from 'gi://Gio';

import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const 对象路径 = '/com/k0kubun/Xremap';
const 接口定义 = `
<node>
  <interface name="com.k0kubun.Xremap">
    <method name="ActiveWindow">
      <arg type="s" direction="out"/>
    </method>
    <method name="WMClass">
      <arg type="s" direction="out"/>
    </method>
    <method name="WMClasses">
      <arg type="s" direction="out"/>
    </method>
  </interface>
</node>`;

/**
 * 焦点窗口的「应用标识」：Wayland 原生应用是 app-id，XWayland 是 WM_CLASS。
 */
function 窗口应用标识(窗口: 窗口接口 | null): string {
    if (!窗口)
        return '';
    const 候选列表 = [
        () => 窗口.get_wm_class(),
        () => 窗口.get_gtk_application_id(),
        () => 窗口.get_sandboxed_app_id(),
        () => 窗口.get_wm_class_instance(),
    ];
    for (const get of 候选列表) {
        try {
            const value = get();
            if (value)
                return value;
        } catch {
            /* 某些窗口类型上这些方法会抛错，忽略 */
        }
    }
    return '';
}

const 焦点桥 = class {
    ActiveWindow(): string {
        const 窗口 = global.display.focus_window;
        return JSON.stringify({
            wm_class: 窗口应用标识(窗口),
            title: 窗口?.get_title?.() ?? '',
        });
    }

    WMClass(): string {
        return 窗口应用标识(global.display.focus_window);
    }

    // 便于排查：列出所有窗口的 app-id
    WMClasses(): string {
        const list = global.get_window_actors()
            .map(演员 => 演员.meta_window)
            .filter((窗口): 窗口 is 窗口接口 => 窗口 != null && !窗口.is_skip_taskbar())
            .map(窗口 => 窗口应用标识(窗口))
            .filter(id => id);
        return JSON.stringify([...new Set(list)].sort());
    }
};

export default class 焦点扩展 extends Extension {
    private _导出对象: DBusExportedObject | null = null;
    enable(): void {
        this._导出对象 = Gio.DBusExportedObject.wrapJSObject(接口定义, new 焦点桥());
        // 注意：在 gnome-shell 进程内导出，因此挂在 org.gnome.Shell 这个总线名下，
        // 与 xremap 期望的调用方式完全一致。
        this._导出对象.export(Gio.DBus.session, 对象路径);
    }

    disable(): void {
        this._导出对象?.unexport();
        this._导出对象 = null;
    }
}
