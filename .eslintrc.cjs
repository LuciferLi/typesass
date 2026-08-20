/** CodexMan 子项目 ESLint 兼容配置。 */
module.exports = {
    extends: ['../.eslintrc.cjs'],
    settings: {
        'import/resolver': {
            typescript: {
                project: `${__dirname}/tsconfig.json`
            }
        }
    },
    rules: {
        // eslint-plugin-import 无法解析 Vue script setup 的隐式默认导出，真实导出由 TypeScript 校验。
        'import/named': 'off',
        // shadcn-vue 组件入口与样式变体按官方模式互相引用，属于受控组件内聚。
        'import/no-cycle': 'off',
        // UI 事件使用 void 明确忽略已自行处理错误的 Promise。
        'no-void': 'off',
        // Tauri 注入到 window 的桥接字段必须保留双下划线协议名。
        'no-underscore-dangle': 'off',
        // 实时字幕必须串行采集和处理音频片段，不能并发执行循环体。
        'no-await-in-loop': 'off'
    }
};
