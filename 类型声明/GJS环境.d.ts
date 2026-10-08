// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Mackey contributors
//
// GJS 运行时与环境的最小类型面：只声明本仓库 JS 代码用到的 API。
// GNOME Shell / GJS 不发布官方 TypeScript 类型，@girs/* 又需要 npm 依赖；
// 这份手写声明由 `cargo run --quiet -- 类型检查`（tsc --checkJs）与编辑器共同消费。

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

/** gnome-shell / GJS 提供的全局对象。 */
declare var global: {
    display: {
        focus_window: 窗口接口 | null;
    };
    get_window_actors(): 窗口演员接口[];
};

/** GJS 提供的全局函数与变量。 */
declare function print(msg?: unknown): void;
declare var ARGV: string[];

declare module 'gi://Gio' {
    export interface DBusReply {
        deepUnpack(): unknown[];
    }

    export interface DBusConnection {
        get_unique_name(): string;
        call_sync(busName: string, objectPath: string, interfaceName: string, methodName: string,
                  parameters: unknown, replyType: unknown, flags: number, timeout: number,
                  cancellable: unknown): DBusReply;
        call(busName: string, objectPath: string, interfaceName: string, methodName: string,
             parameters: unknown, replyType: unknown, flags: number, timeout: number,
             cancellable: unknown,
             callback: (source: DBusConnection, result: unknown) => void): void;
        call_finish(result: unknown): DBusReply;
    }

    export interface DBusExportedObject {
        export(bus: unknown, path: string): void;
        unexport(): void;
    }

    export interface Gio命名空间 {
        SubprocessFlags: { STDOUT_PIPE: number; STDERR_PIPE: number };
        Subprocess: { "new"(argv: string[], flags: number): Subprocess };
        DBusExportedObject: {
            wrapJSObject(xml: string, obj: object): DBusExportedObject;
        };
        DBus: {
            session: DBusConnection;
        };
        DBusCallFlags: { NONE: number };
    }

    export interface Subprocess {
        communicate_utf8_async(input: string | null, cancellable: unknown,
            callback: (child: Subprocess, result: unknown) => void): void;
        communicate_utf8_finish(result: unknown): [boolean, string | null, string | null];
        get_successful(): boolean;
    }

    const Gio: Gio命名空间;
    export default Gio;
}

declare module 'gi://GLib' {
    export interface MainLoop {
        run(): void;
        quit(): void;
    }

    export interface GLib命名空间 {
        Variant: new(signature: string, value: unknown) => unknown;
        MainLoop: {
            "new"(priority: unknown, running: boolean): MainLoop;
        };
    }

    const GLib: GLib命名空间;
    export default GLib;
}

declare module 'system' {
    const System: { exit(code?: number): void };
    export default System;
}

declare module 'resource:///org/gnome/shell/extensions/extension.js' {
    export class Extension {
        constructor(metadata?: unknown);
    }
}
