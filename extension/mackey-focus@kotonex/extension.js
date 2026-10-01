// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Mackey contributors
//
// 接口契约源自 xremap-gnome（https://github.com/xremap/xremap-gnome,
// Copyright (C) 2022 Takashi Kokubun, GPLv2+）；本文件是对该公开 D-Bus 接口的独立实现。
//
// xremap Bridge —— 只做一件事：把「当前焦点窗口的 wm_class / title」通过 D-Bus 暴露出去。
//
// 背景：xremap 在 GNOME Wayland 上做 application 匹配时，会调用
//   org.gnome.Shell:/com/k0kubun/Xremap -> com.k0kubun.Xremap.ActiveWindow() -> s(JSON)
// 上游扩展 xremap@k0kubun.com 在 GNOME 45+ 上已失效（xremap 启动时会打印
// `application-client: GNOME (supported: false)`），导致「终端/应用差异化映射」全部失效。
//
// 本扩展用**同一份 D-Bus 契约**重新实现它，因此无需给 xremap 打补丁。
// 安全边界：仅读取焦点窗口元数据；不抓键、不注入按键、不写任何文件。

// @ts-check
import Gio from 'gi://Gio';

import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const OBJECT_PATH = '/com/k0kubun/Xremap';
const IFACE_XML = `
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
 * @param {MetaWindowLike | null} win
 * @returns {string}
 */
function windowAppId(win) {
    if (!win)
        return '';
    const candidates = [
        () => win.get_wm_class(),
        () => win.get_gtk_application_id(),
        () => win.get_sandboxed_app_id(),
        () => win.get_wm_class_instance(),
    ];
    for (const get of candidates) {
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

const Bridge = class {
    ActiveWindow() {
        const win = global.display.focus_window;
        return JSON.stringify({
            wm_class: windowAppId(win),
            title: win?.get_title?.() ?? '',
        });
    }

    WMClass() {
        return windowAppId(global.display.focus_window);
    }

    // 便于排查：列出所有窗口的 app-id
    WMClasses() {
        const list = global.get_window_actors()
            .map(actor => actor.meta_window)
            .filter(win => win && !win.is_skip_taskbar())
            .map(win => windowAppId(win))
            .filter(id => id);
        return JSON.stringify([...new Set(list)].sort());
    }
};

export default class XremapCompatExtension extends Extension {
    enable() {
        this._exported = Gio.DBusExportedObject.wrapJSObject(IFACE_XML, new Bridge());
        // 注意：在 gnome-shell 进程内导出，因此挂在 org.gnome.Shell 这个总线名下，
        // 与 xremap 期望的调用方式完全一致。
        this._exported.export(Gio.DBus.session, OBJECT_PATH);
    }

    disable() {
        this._exported?.unexport();
        this._exported = null;
    }
}
