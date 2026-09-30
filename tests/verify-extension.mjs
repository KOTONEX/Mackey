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
// 用法：gjs -m tests/verify-extension.mjs <被改写过的 extension.js 路径>
// @ts-check
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import System from 'system';

const PATH = '/com/k0kubun/Xremap';
const IFACE = 'com.k0kubun.Xrmap'.replace('Xrmap', 'Xremap');

let failures = 0;
/**
 * @param {string} name
 * @param {boolean} cond
 * @param {string} [extra]
 */
function check(name, cond, extra = '') {
    if (cond) {
        print(`  ✓ ${name}`);
    } else {
        failures++;
        print(`  ✗ ${name} ${extra}`);
    }
}

/**
 * @param {string} busName
 * @param {string} method
 */
function call(busName, method) {
    // 注意：被调用的对象就在本进程里，所以必须用**异步**调用并驱动主循环，
    // 否则同步 call_sync 会把自己阻塞死（进来的方法调用永远没机会被派发）。
    const loop = GLib.MainLoop.new(null, false);
    /** @type {any} */
    let reply = null;
    /** @type {any} */
    let error = null;
    Gio.DBus.session.call(
        busName, PATH, IFACE, method, null, null,
        Gio.DBusCallFlags.NONE, 2000, null,
        (source, result) => {
            try {
                reply = source.call_finish(result);
            } catch (e) {
                error = e;
            }
            loop.quit();
        });
    loop.run();
    if (error)
        throw error;
    return reply.deepUnpack()[0];
}

// ---- 桩：shell 提供的全局对象 -------------------------------------------------
/** @type {MetaWindowLike} */
const fakeWindow = {
    get_wm_class: () => 'TestTerminal',
    get_gtk_application_id: () => null,
    get_sandboxed_app_id: () => null,
    get_wm_class_instance: () => null,
    get_title: () => 'bash — /tmp',
    is_skip_taskbar: () => false,
};

globalThis.global = {
    display: { focus_window: fakeWindow },
    get_window_actors: () => [{ meta_window: fakeWindow }],
};

// ---- 加载真实扩展源码 ---------------------------------------------------------
const target = ARGV[0];
if (!target)
    throw new Error('用法: gjs -m tests/verify-extension.mjs <extension.js 路径>');

print(`加载：${target}`);
const module = await import(`file://${target}`);
check('模块有默认导出', typeof module.default === 'function');

const instance = new module.default({});
check('实例可构造', !!instance);

// ---- enable() 与 D-Bus 契约 ---------------------------------------------------
instance.enable();
const unique = Gio.DBus.session.get_unique_name();
print(`  本进程总线名：${unique}`);

let payload;
try {
    payload = call(unique, 'ActiveWindow');
    check('ActiveWindow 可被 D-Bus 调用', true);
} catch (e) {
    check('ActiveWindow 可被 D-Bus 调用', false, `→ ${e.message}`);
}

let parsed = null;
try {
    parsed = JSON.parse(payload);
} catch (e) {
    check('ActiveWindow 返回可解析 JSON', false, `→ ${payload}`);
}
const isObject = parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed);
check('ActiveWindow 返回非空 JSON 对象', isObject, `→ ${payload}`);
if (isObject) {
    check('JSON 含 wm_class 字段', parsed.wm_class === 'TestTerminal', `→ ${JSON.stringify(parsed)}`);
    check('JSON 含 title 字段', parsed.title === 'bash — /tmp', `→ ${JSON.stringify(parsed)}`);
}

try {
    check('WMClass 返回纯类名', call(unique, 'WMClass') === 'TestTerminal');
} catch (e) {
    check('WMClass 返回纯类名', false, `→ ${e.message}`);
}

try {
    const list = JSON.parse(call(unique, 'WMClasses'));
    check('WMClasses 返回去重列表', Array.isArray(list) && list.join() === 'TestTerminal',
        `→ ${JSON.stringify(list)}`);
} catch (e) {
    check('WMClasses 返回去重列表', false, `→ ${e.message}`);
}

// 焦点窗口缺失时应返回空类名，而不是抛错（xremap 会退到兜底 keymap）
globalThis.global.display.focus_window = null;
try {
    const empty = JSON.parse(call(unique, 'ActiveWindow'));
    check('无焦点窗口时返回空 wm_class', empty.wm_class === '' && empty.title === '',
        `→ ${JSON.stringify(empty)}`);
} catch (e) {
    check('无焦点窗口时返回空 wm_class', false, `→ ${e.message}`);
}

// ---- disable() ---------------------------------------------------------------
instance.disable();
let gone = false;
try {
    call(unique, 'ActiveWindow');
} catch (e) {
    gone = e.message.includes('UnknownMethod') || e.message.includes('does not exist') ||
        e.message.includes('No such');
}
check('disable() 撤掉 D-Bus 导出', gone);

print(failures === 0 ? '\n✓ 扩展契约验证通过' : `\n✗ ${failures} 项未通过`);
System.exit(failures === 0 ? 0 : 1);
