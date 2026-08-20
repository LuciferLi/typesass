import { constants } from 'node:fs';
import { access, cp, mkdir, mkdtemp, readFile, rename, rm, symlink } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import process from 'node:process';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';

/** 当前脚本目录，用于稳定解析 CodexMan 项目根目录。 */
const scriptDirectory = dirname(fileURLToPath(import.meta.url));
/** CodexMan 项目根目录。 */
const projectRoot = resolve(scriptDirectory, '..');
/** Tauri 配置文件路径。 */
const tauriConfigPath = join(projectRoot, 'src-tauri', 'tauri.conf.json');
/** Tauri macOS bundle 输出根目录。 */
const tauriBundleDirectory = join(projectRoot, 'src-tauri', 'target', 'release', 'bundle');
/** 官网静态下载目录，发布前会把已公证 dmg 复制到这里。 */
const websiteDownloadDirectory = join(projectRoot, 'website', 'downloads');
/** Apple 公证钥匙串 profile，来自本机 notarytool store-credentials。 */
const notaryProfile = process.env.CODEXMAN_NOTARY_PROFILE || 'codexman-notary';
/** macOS Developer ID Application 签名身份，用于补签备用流程生成的 DMG。 */
const macSigningIdentity =
    process.env.CODEXMAN_MAC_SIGN_IDENTITY || 'Developer ID Application: Tamba Trading Co., Ltd. (9VKQ2P8P6N)';

/**
 * 读取并解析 Tauri 配置。
 * 流程：只从 tauri.conf.json 读取产品名和版本，避免 package.json 与 Tauri 版本分叉。
 * 参数：无。
 * 返回：包含产品名和版本的对象。
 * 异常/边界：产品名必须固定为 CodexMan；版本必须是非空字符串，否则阻止发布。
 */
async function readTauriReleaseConfig() {
    const rawConfig = await readFile(tauriConfigPath, 'utf8');
    const config = JSON.parse(rawConfig);
    const productName = String(config.productName || '');
    const version = String(config.version || '');
    if (productName !== 'CodexMan') {
        throw new Error(`Tauri productName 必须为 CodexMan，当前为 ${productName || '空'}`);
    }
    if (!version) {
        throw new Error('Tauri version 不能为空。');
    }
    return { productName, version };
}

/**
 * 判断路径是否存在且可读取。
 * 流程：使用 access 做只读探测。
 * 参数：path 为待检查路径。
 * 返回：可读取时为 true，否则为 false。
 * 异常/边界：不存在和权限不足都按 false 处理，由调用方输出上下文错误。
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
 * 执行发布命令。
 * 流程：继承当前终端输出，命令非零退出时抛出稳定错误。
 * 参数：command 为命令名；args 为参数数组；options 可覆盖工作目录和环境变量。
 * 返回：命令成功退出后完成。
 * 异常/边界：命令启动失败、信号中断或退出码非 0 都阻止后续发布步骤。
 */
function run(command, args, options = {}) {
    return new Promise((resolvePromise, reject) => {
        const child = spawn(command, args, {
            cwd: options.cwd || projectRoot,
            env: { ...process.env, ...(options.env || {}) },
            stdio: 'inherit'
        });
        child.once('error', reject);
        child.once('exit', (code, signal) => {
            if (code === 0) resolvePromise();
            else reject(new Error(`${command} ${args.join(' ')} 失败：exit=${code ?? 'null'} signal=${signal ?? 'none'}`));
        });
    });
}

/**
 * 执行需要读取 stdout 的发布命令。
 * 流程：收集 stdout/stderr，成功时返回 stdout；失败时把 stderr 合入错误，便于定位 hdiutil 这类静默失败。
 * 参数：command 为命令名；args 为参数数组；options 可覆盖工作目录和环境变量。
 * 返回：命令 stdout 文本。
 * 异常/边界：不会把 stdout/stderr 写入日志文件；调用方不得传入包含密钥的命令参数。
 */
function runCapture(command, args, options = {}) {
    return new Promise((resolvePromise, reject) => {
        const child = spawn(command, args, {
            cwd: options.cwd || projectRoot,
            env: { ...process.env, ...(options.env || {}) },
            stdio: ['ignore', 'pipe', 'pipe']
        });
        let stdout = '';
        let stderr = '';
        child.stdout.on('data', (chunk) => {
            stdout += chunk.toString();
        });
        child.stderr.on('data', (chunk) => {
            stderr += chunk.toString();
        });
        child.once('error', reject);
        child.once('exit', (code, signal) => {
            if (code === 0) {
                resolvePromise(stdout);
                return;
            }
            reject(
                new Error(
                    `${command} ${args.join(' ')} 失败：exit=${code ?? 'null'} signal=${signal ?? 'none'}\n${stderr || stdout}`
                )
            );
        });
    });
}

/**
 * 解析当前 macOS 架构对应的 Tauri dmg 架构后缀。
 * 流程：把 Node 架构映射为 Tauri dmg 命名使用的后缀。
 * 参数：无。
 * 返回：aarch64 或 x64。
 * 异常/边界：未知架构直接阻止发布，避免上传不可识别文件名。
 */
function resolveDmgArchitecture() {
    if (process.arch === 'arm64') return 'aarch64';
    if (process.arch === 'x64') return 'x64';
    throw new Error(`暂不支持当前架构发布 macOS dmg：${process.arch}`);
}

/**
 * 查找本次 Tauri 构建输出的 dmg。
 * 流程：优先使用固定命名；若不存在则扫描 dmg 目录并拒绝旧 typesass 命名产物。
 * 参数：version 为 Tauri 版本号。
 * 返回：可用于公证的 dmg 绝对路径。
 * 异常/边界：没有 dmg、多个 dmg 或命中 typesass 旧命名时均阻止发布。
 */
async function resolveBuiltDmg(version) {
    const architecture = resolveDmgArchitecture();
    const expectedName = `codexman_${version}_${architecture}.dmg`;
    const expectedPath = join(tauriBundleDirectory, 'dmg', expectedName);
    if (await exists(expectedPath)) return expectedPath;

    const { readdir } = await import('node:fs/promises');
    const dmgDirectory = join(tauriBundleDirectory, 'dmg');
    const entries = await readdir(dmgDirectory).catch(() => []);
    const dmgEntries = entries.filter((name) => name.endsWith('.dmg'));
    if (dmgEntries.some((name) => name.toLowerCase().includes('typesass'))) {
        throw new Error(`发现旧 typesass 命名 dmg，已拒绝发布：${dmgEntries.join(', ')}`);
    }
    if (dmgEntries.length !== 1) {
        throw new Error(`未找到唯一 dmg 产物，当前为：${dmgEntries.join(', ') || '空'}`);
    }
    const actualPath = join(dmgDirectory, dmgEntries[0]);
    const normalizedPath = expectedPath;
    await cp(actualPath, normalizedPath);
    return normalizedPath;
}

/**
 * 解析 hdiutil attach 输出中的设备与挂载目录。
 * 流程：设备取第一条 `/dev/` 行，挂载目录取包含 Apple_HFS 的最后一列。
 * 参数：output 为 hdiutil attach stdout。
 * 返回：设备路径和挂载目录。
 * 异常/边界：解析不到任一字段时阻止继续写入镜像，避免误操作本机其它目录。
 */
function parseAttachOutput(output) {
    const lines = output.split('\n').map((line) => line.trim()).filter(Boolean);
    const device = lines.find((line) => line.startsWith('/dev/'))?.split(/\s+/)[0] || '';
    const hfsLine = lines.find((line) => line.includes('Apple_HFS'));
    const mountDir = hfsLine?.split(/\s+/).at(-1) || '';
    if (!device || !mountDir) {
        throw new Error(`无法解析 hdiutil attach 输出：${output}`);
    }
    return { device, mountDir };
}

/**
 * 使用同名 App workaround 生成 CodexMan DMG。
 * 流程：先把已签名 App 复制为临时 Payload.app 规避 hdiutil 对卷名 CodexMan + CodexMan.app 的权限错误，挂载后再改回 CodexMan.app 并压缩为最终 DMG。
 * 参数：version 为 Tauri 版本号。
 * 返回：生成的 dmg 绝对路径。
 * 异常/边界：仅在 Tauri 已成功生成签名 `.app` 后使用；失败时会尽力卸载临时镜像并清理临时目录。
 */
async function buildDmgWithSameNameWorkaround(version) {
    const architecture = resolveDmgArchitecture();
    const appPath = join(tauriBundleDirectory, 'macos', 'CodexMan.app');
    if (!(await exists(appPath))) {
        throw new Error('Tauri DMG 失败后未找到 CodexMan.app，无法执行备用 DMG 生成。');
    }
    const dmgDirectory = join(tauriBundleDirectory, 'dmg');
    const finalDmgPath = join(dmgDirectory, `codexman_${version}_${architecture}.dmg`);
    const temporaryDirectory = await mkdtemp(join(tmpdir(), 'codexman-dmg-work-'));
    const stagingDirectory = join(temporaryDirectory, 'staging');
    const readWriteDmgPath = join(temporaryDirectory, 'codexman-rw.dmg');
    let attachedDevice = '';
    try {
        await mkdir(dmgDirectory, { recursive: true });
        await mkdir(stagingDirectory, { recursive: true });
        await run('ditto', [appPath, join(stagingDirectory, 'Payload.app')]);
        await symlink('/Applications', join(stagingDirectory, 'Applications'));
        await run('hdiutil', [
            'create',
            '-srcfolder',
            stagingDirectory,
            '-volname',
            'CodexMan',
            '-fs',
            'HFS+',
            '-format',
            'UDRW',
            readWriteDmgPath
        ]);
        const attachOutput = await runCapture('hdiutil', [
            'attach',
            '-mountrandom',
            tmpdir(),
            '-readwrite',
            '-noverify',
            '-noautoopen',
            '-nobrowse',
            readWriteDmgPath
        ]);
        const { device, mountDir } = parseAttachOutput(attachOutput);
        attachedDevice = device;
        await rename(join(mountDir, 'Payload.app'), join(mountDir, 'CodexMan.app'));
        await run('hdiutil', ['detach', attachedDevice]);
        attachedDevice = '';
        await rm(finalDmgPath, { force: true });
        await run('hdiutil', ['convert', readWriteDmgPath, '-format', 'UDZO', '-o', finalDmgPath]);
        return finalDmgPath;
    } finally {
        if (attachedDevice) {
            await run('hdiutil', ['detach', attachedDevice]).catch(() => undefined);
        }
        await rm(temporaryDirectory, { recursive: true, force: true });
    }
}

/**
 * 对 DMG 产物执行 Developer ID 签名。
 * 流程：先强制替换已有签名，再交给 notarytool 公证，确保 Gatekeeper 能识别磁盘镜像自身签名。
 * 参数：dmgPath 为待发布 DMG 绝对路径。
 * 返回：签名完成后返回。
 * 异常/边界：签名身份可通过 CODEXMAN_MAC_SIGN_IDENTITY 覆盖；签名失败会阻止发布，避免上传不可安装产物。
 */
async function signDmg(dmgPath) {
    await run('codesign', ['--force', '--sign', macSigningIdentity, dmgPath]);
    await run('codesign', ['--verify', '--verbose=4', dmgPath]);
}

/**
 * 执行 CodexMan Tauri macOS 发布闭环。
 * 流程：清理旧 bundle、构建 dmg、公证、贴票、Gatekeeper 验证，并复制到官网下载目录。
 * 参数：无。
 * 返回：发布产物路径。
 * 异常/边界：这是 Tauri 专用流程，禁止用 Electron builder、electron-notarize 或 Electron updater 替代。
 */
async function main() {
    const { version } = await readTauriReleaseConfig();
    const architecture = resolveDmgArchitecture();
    const releaseFileName = `codexman_${version}_${architecture}.dmg`;
    const websiteDmgPath = join(websiteDownloadDirectory, releaseFileName);

    await rm(tauriBundleDirectory, { recursive: true, force: true });
    await rm(websiteDmgPath, { force: true });

    await run('npm', ['run', 'build']);
    try {
        await run('npx', ['tauri', 'build', '--bundles', 'dmg']);
    } catch (error) {
        process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
        process.stderr.write('Tauri 原生 DMG 生成失败，尝试使用 CodexMan 同名 App workaround 生成 DMG。\n');
        await buildDmgWithSameNameWorkaround(version);
    }

    const dmgPath = await resolveBuiltDmg(version);
    await signDmg(dmgPath);
    await run('xcrun', ['notarytool', 'submit', dmgPath, '--keychain-profile', notaryProfile, '--wait']);
    await run('xcrun', ['stapler', 'staple', dmgPath]);
    await run('xcrun', ['stapler', 'validate', dmgPath]);
    await run('spctl', ['-a', '-vvv', '-t', 'install', dmgPath]);

    await mkdir(websiteDownloadDirectory, { recursive: true });
    await cp(dmgPath, websiteDmgPath);
    process.stdout.write(`CodexMan macOS dmg 已发布到官网目录：${websiteDmgPath}\n`);
}

await main();
