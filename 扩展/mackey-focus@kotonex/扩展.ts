// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Mackey contributors
// D-Bus 接口来源：xremap-gnome（Copyright (C) 2022 Takashi Kokubun，GPL-2.0-or-later）。

import Gio from 'gi://Gio';

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
        } catch {}
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
    private _导出对象: Gio.DBusExportedObject | null = null;
    enable(): void {
        this._导出对象 = Gio.DBusExportedObject.wrapJSObject(接口定义, new 焦点桥());
        this._导出对象.export(Gio.DBus.session, 对象路径);
    }

    disable(): void {
        this._导出对象?.unexport();
        this._导出对象 = null;
    }
}
