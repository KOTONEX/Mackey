// SPDX-License-Identifier: AGPL-3.0-or-later
// 只替代 GNOME Shell 的 Extension 基类，保留真实扩展和 D-Bus 行为。
export class Extension {
    constructor(public metadata: unknown) {}
}
