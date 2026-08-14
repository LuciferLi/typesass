import { constants } from 'node:fs';
import { access, cp, mkdir, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

/** 当前脚本目录，用于稳定解析 AiTool 项目根目录。 */
const scriptDirectory = dirname(fileURLToPath(import.meta.url));
/** AiTool 项目根目录。 */
const projectRoot = resolve(scriptDirectory, '..');
/** Chrome 插件唯一源码目录。 */
const sourceDirectory = join(projectRoot, 'codexManExtension');
/** 前端静态下载目录，Vite 构建时会原样复制其中的 ZIP。 */
const downloadDirectory = join(projectRoot, 'public', 'downloads');
/** 浏览器插件 ZIP 产物路径，同时供 Tauri include_bytes 打入桌面包。 */
const outputZipPath = join(downloadDirectory, 'typesass-extension.zip');
/** ZIP 内部根目录名，用户解压后直接加载该目录。 */
const extensionDirectoryName = 'typesass-extension';

/**
 * 判断路径是否存在且可读取。
 * 流程：使用 fs access 做只读探测，避免后续复制或打包时才暴露模糊错误。
 * 参数：path 为待检查绝对路径。
 * 返回：存在且可读时返回 true，否则返回 false。
 * 异常/边界：权限不足与不存在统一视为不可用，由调用方输出稳定构建错误。
 */
async function exists(path) {
    try {
        await access(path, constants.R_OK);
        return true;
    } catch {
        return false;
    }
}

/**
 * 执行 ZIP 打包命令。
 * 流程：在临时目录中调用系统 zip，将内部根目录打包到 public/downloads 固定产物。
 * 参数：temporaryRoot 为本次构建临时根目录。
 * 返回：zip 命令成功退出后完成。
 * 异常/边界：zip 不存在、被信号中断或非零退出均拒绝构建，避免发布旧包。
 */
function runZip(temporaryRoot) {
    return new Promise((resolvePromise, reject) => {
        const child = spawn('zip', ['-X', '-r', outputZipPath, extensionDirectoryName], {
            cwd: temporaryRoot,
            stdio: 'inherit'
        });
        child.once('error', reject);
        child.once('exit', (code, signal) => {
            if (code === 0) resolvePromise();
            else reject(new Error(`浏览器插件 ZIP 构建失败：exit=${code ?? 'null'} signal=${signal ?? 'none'}`));
        });
    });
}

/**
 * 构建可供页面下载和桌面端导出的 Chrome 插件 ZIP。
 * 流程：校验唯一源码目录，复制到临时 typesass-extension 根目录，删除旧 ZIP 后重新压缩。
 * 参数：无。
 * 返回：成功时无业务返回值。
 * 异常/边界：不会读取 public/downloads 下的解压副本，防止源码与下载包双写分叉。
 */
async function main() {
    if (!(await exists(join(sourceDirectory, 'manifest.json')))) {
        throw new Error(`缺少浏览器插件源码 manifest：${join(sourceDirectory, 'manifest.json')}`);
    }

    const temporaryRoot = await mkdtemp(join(tmpdir(), 'aitool-browser-extension-'));
    try {
        const temporaryExtensionDirectory = join(temporaryRoot, extensionDirectoryName);
        await mkdir(downloadDirectory, { recursive: true });
        await rm(outputZipPath, { force: true });
        await cp(sourceDirectory, temporaryExtensionDirectory, {
            recursive: true,
            preserveTimestamps: true,
            filter: (sourcePath) => basename(sourcePath) !== '.DS_Store'
        });
        await runZip(temporaryRoot);
        process.stdout.write(`浏览器插件 ZIP 已生成：${outputZipPath}\n`);
    } finally {
        await rm(temporaryRoot, { recursive: true, force: true });
    }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    await main();
}
