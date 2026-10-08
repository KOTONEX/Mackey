// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Mackey contributors
// 隔离带测试：验证 extension.js 能被加载、D-Bus 契约能真的被调用。
//
// 为什么需要它：GNOME 在 Wayland 下不会热扫描扩展目录，而嵌套/headless 的
// gnome-shell 又会因为抢占 logind 会话（EBUSY）/段错误而跑不起来。
// 所以这里加载**真实的 extension.js 源码**，只桩化 shell 提供的全局对象
// （resource:///org/gnome/shell/extensions/extension.js 与 global.display），
// 然后用真实 D-Bus 往返（自调用）验证：
//   1. 模块能解析、默认导出的类能实例化；
//   2. enable() 会在 /com/k0kubun/Xremap 导出 com.k0kubun.Xremap；
//   3. ActiveWindow 返回 {"wm_class","title"} 形状的 JSON，且能读到焦点窗口；
//   4. WMClass 返回纯类名、WMClasses 返回去重列表；
//   5. disable() 会撤掉导出。
//
// 用法：gjs -m 测试/扩展契约.mjs <被改写过的 extension.js 路径>
// @ts-check
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import System from 'system';

const 对象路径 = '/com/k0kubun/Xremap';
const 接口名称 = 'com.k0kubun.Xrmap'.replace('Xrmap', 'Xremap');

let 失败数量 = 0;
/** @param {unknown} 错误 */
function 错误说明(错误) {
    return 错误 instanceof Error ? 错误.message : String(错误);
}
/**
 * @param {string} name
 * @param {boolean} cond
 * @param {string} [extra]
 */
function 检查(name, cond, extra = '') {
    if (cond) {
        print(`  ✓ ${name}`);
    } else {
        失败数量++;
        print(`  ✗ ${name} ${extra}`);
    }
}

/**
 * @param {string} busName
 * @param {string} method
 */
function 调用(busName, method) {
    // 注意：被调用的对象就在本进程里，所以必须用**异步**调用并驱动主循环，
    // 否则同步 call_sync 会把自己阻塞死（进来的方法调用永远没机会被派发）。
    const loop = GLib.MainLoop.new(null, false);
    /** @type {{回复: import('gi://Gio').DBusReply | null, 错误: unknown}} */
    const 结果 = {回复: null, 错误: null};
    Gio.DBus.session.call(
        busName, 对象路径, 接口名称, method, null, null,
        Gio.DBusCallFlags.NONE, 2000, null,
        (source, result) => {
            try {
                结果.回复 = source.call_finish(result);
            } catch (e) {
                结果.错误 = e;
            }
            loop.quit();
        });
    loop.run();
    if (结果.错误)
        throw 结果.错误;
    if (!结果.回复)
        throw new Error('D-Bus 未返回回复');
    const 文本 = 结果.回复.deepUnpack()[0];
    if (typeof 文本 !== 'string')
        throw new Error('D-Bus 回复必须是字符串');
    return 文本;
}

// ---- 桩：shell 提供的全局对象 -------------------------------------------------
/** @type {窗口接口} */
const 模拟窗口 = {
    get_wm_class: () => 'TestTerminal',
    get_gtk_application_id: () => null,
    get_sandboxed_app_id: () => null,
    get_wm_class_instance: () => null,
    get_title: () => 'bash — /tmp',
    is_skip_taskbar: () => false,
};

globalThis.global = {
    display: { focus_window: 模拟窗口 },
    get_window_actors: () => [{ meta_window: 模拟窗口 }],
};

// ---- 加载真实扩展源码 ---------------------------------------------------------
const 目标文件 = ARGV[0];
if (!目标文件)
    throw new Error('用法: gjs -m 测试/扩展契约.mjs <extension.js 路径>');

print(`加载：${目标文件}`);
const module = await import(`file://${目标文件}`);
检查('模块有默认导出', typeof module.default === 'function');

const 实例 = new module.default({});
检查('实例可构造', !!实例);

// ---- enable() 与 D-Bus 契约 ---------------------------------------------------
实例.enable();
// 隔离总线没有真实 GNOME Shell，取得其名称以便 Rust 客户端查找。
const 名称归属 = Gio.DBus.session.call_sync(
    'org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus',
    'RequestName', new GLib.Variant('(su)', ['org.gnome.Shell', 0]), null,
    Gio.DBusCallFlags.NONE, 2000, null).deepUnpack()[0];
检查('隔离会话取得 org.gnome.Shell 名称', 名称归属 === 1 || 名称归属 === 4);
const 总线名称 = Gio.DBus.session.get_unique_name();
print(`  本进程总线名：${总线名称}`);

let 载荷 = "";
try {
    载荷 = 调用(总线名称, 'ActiveWindow');
    检查('ActiveWindow 可被 D-Bus 调用', true);
} catch (e) {
    检查('ActiveWindow 可被 D-Bus 调用', false, `→ ${错误说明(e)}`);
}

let 解析结果 = null;
try {
    解析结果 = JSON.parse(载荷);
} catch (e) {
    检查('ActiveWindow 返回可解析 JSON', false, `→ ${载荷}`);
}
const 是对象 = 解析结果 !== null && typeof 解析结果 === 'object' && !Array.isArray(解析结果);
检查('ActiveWindow 返回非空 JSON 对象', 是对象, `→ ${载荷}`);
if (是对象) {
    检查('JSON 含 wm_class 字段', 解析结果.wm_class === 'TestTerminal', `→ ${JSON.stringify(解析结果)}`);
    检查('JSON 含 title 字段', 解析结果.title === 'bash — /tmp', `→ ${JSON.stringify(解析结果)}`);
}

// 独立 Rust 进程调用该对象期间继续派发 GJS 主循环。
const 桥进程 = Gio.Subprocess.new(
    [ARGV[1], '焦点桥', '--后端', 'k0kubun', '--测试'],
    Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE);
const 桥主循环 = GLib.MainLoop.new(null, false);
桥进程.communicate_utf8_async(null, null, (child, result) => {
    try {
        const [, stdout, stderr] = child.communicate_utf8_finish(result);
        检查('Rust 焦点桥 可调用真实扩展', child.get_successful(), stderr || '');
        const info = JSON.parse(stdout || 'null');
        检查('Rust 获得准确焦点', info?.wm_class === 'TestTerminal' && info?.title === 'bash — /tmp');
    } catch (e) {
        检查('Rust D-Bus 互通', false, 错误说明(e));
    }
    桥主循环.quit();
});
桥主循环.run();

try {
    检查('WMClass 返回纯类名', 调用(总线名称, 'WMClass') === 'TestTerminal');
} catch (e) {
    检查('WMClass 返回纯类名', false, `→ ${错误说明(e)}`);
}

try {
    const list = JSON.parse(调用(总线名称, 'WMClasses'));
    检查('WMClasses 返回去重列表', Array.isArray(list) && list.join() === 'TestTerminal',
        `→ ${JSON.stringify(list)}`);
} catch (e) {
    检查('WMClasses 返回去重列表', false, `→ ${错误说明(e)}`);
}

// 焦点窗口缺失时应返回空类名，而不是抛错（xremap 会退到兜底 keymap）
globalThis.global.display.focus_window = null;
try {
    const empty = JSON.parse(调用(总线名称, 'ActiveWindow'));
    检查('无焦点窗口时返回空 wm_class', empty.wm_class === '' && empty.title === '',
        `→ ${JSON.stringify(empty)}`);
} catch (e) {
    检查('无焦点窗口时返回空 wm_class', false, `→ ${错误说明(e)}`);
}

// ---- disable() ---------------------------------------------------------------
实例.disable();
let 已撤销 = false;
try {
    调用(总线名称, 'ActiveWindow');
} catch (e) {
    已撤销 = 错误说明(e).includes('UnknownMethod') || 错误说明(e).includes('does not exist') ||
        错误说明(e).includes('No such');
}
检查('disable() 撤掉 D-Bus 导出', 已撤销);

print(失败数量 === 0 ? '\n✓ 扩展契约验证通过' : `\n✗ ${失败数量} 项未通过`);
System.exit(失败数量 === 0 ? 0 : 1);
