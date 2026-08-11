import { access, chmod, cp, mkdir, mkdtemp, readFile, rm } from 'node:fs/promises';
import { constants } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

/** 当前构建脚本目录，用于稳定解析 aiTool 根目录。 */
const scriptDirectory = dirname(fileURLToPath(import.meta.url));
/** aiTool 项目根目录。 */
const projectRoot = resolve(scriptDirectory, '..');
/** FastAPI sidecar 的 PyInstaller 入口，由 server 模块维护。 */
const entryFile = join(projectRoot, 'server', 'sidecar_main.py');
/** Tauri externalBin 输出目录。 */
const binaryDirectory = join(projectRoot, 'src-tauri', 'binaries');
/** 锁定全部 sidecar 构建传递依赖及哈希的唯一文件。 */
const buildRequirements = join(projectRoot, 'server', 'requirements-build.lock');

/** macOS Sidecar 构建允许探测的 CPython 3.9 候选，按确定性优先级排列。 */
const macBuildPythonCandidates = [
    '/usr/bin/python3',
    '/Applications/Xcode.app/Contents/Developer/usr/bin/python3',
    'python3.9',
    'python3'
];

/**
 * 判断文件是否存在且可读取。
 * 流程：使用 fs access 执行只读探测。
 * 参数：path 为待检查绝对路径。
 * 返回：存在且可读时为 true，否则为 false。
 * 异常/边界：权限不足与不存在统一返回 false，不吞掉后续构建命令错误。
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
 * 从校验后的基础解释器派生一次性 sidecar 构建环境。
 * 流程：选择显式 AITOOL_PYTHON，或自动寻找合规 CPython 3.9，再在本次构建临时目录创建全新 venv；随后校验 venv 解释器，并以隔离、哈希校验和纯二进制模式安装唯一锁文件。
 * 参数：temporaryRoot 为本次构建独占临时目录；runCommand 为命令执行器，生产环境使用 run，测试可注入只记录命令的执行器。
 * 返回：只供本次 PyInstaller 使用的隔离 Python 绝对路径。
 * 异常/边界：显式解释器只作为 venv 基础来源，不能直接安装依赖或执行 PyInstaller；缺少锁文件、解释器不合规、venv 未生成 Python 或锁安装失败均阻止构建，临时环境由 main 的 finally 清理。
 */
export async function prepareIsolatedPython(temporaryRoot, runCommand = run) {
    if (!(await exists(buildRequirements))) {
        throw new Error(`缺少 sidecar 锁定构建依赖：${buildRequirements}`);
    }
    const basePython = await resolveBuildPython(runCommand);

    const environmentDirectory = join(temporaryRoot, 'build-environment');
    await runCommand(basePython, ['-I', '-m', 'venv', environmentDirectory]);
    const python = process.platform === 'win32'
        ? join(environmentDirectory, 'Scripts', 'python.exe')
        : join(environmentDirectory, 'bin', 'python');
    if (!(await exists(python))) {
        throw new Error(`隔离 sidecar 构建环境未生成 Python：${python}`);
    }
    await validatePython(python, runCommand);
    await runCommand(python, [
        '-I', '-m', 'pip', 'install', '--isolated', '--disable-pip-version-check',
        '--require-hashes', '--only-binary=:all:', '--requirement', buildRequirements
    ]);
    return python;
}

/**
 * 解析 Sidecar 构建使用的 CPython 3.9 基础解释器。
 * 流程：存在 AITOOL_PYTHON 时只校验该显式值；否则依次校验平台候选，并返回首个合规解释器。
 * 参数：runCommand 为可注入的命令执行器；candidates 用于测试候选回退，生产环境使用当前平台的固定候选。
 * 返回：通过 CPython 3.9 校验的命令或绝对路径。
 * 异常/边界：显式值必须是非空绝对路径且版本合规，否则禁止静默回退；自动探测全部失败时列出候选和修复方式，不接受其他 Python 版本。
 */
export async function resolveBuildPython(
    runCommand = run,
    candidates = process.platform === 'darwin' ? macBuildPythonCandidates : ['python3.9', 'python3']
) {
    const hasExplicitPython = Object.hasOwn(process.env, 'AITOOL_PYTHON');
    if (hasExplicitPython) {
        const explicitPython = process.env.AITOOL_PYTHON?.trim() ?? '';
        if (!explicitPython || !isAbsolute(explicitPython)) {
            throw new Error('AITOOL_PYTHON 必须是 CPython 3.9 可执行文件的非空绝对路径。');
        }
        try {
            await validatePython(explicitPython, runCommand);
            return explicitPython;
        } catch (cause) {
            throw new Error(
                `AITOOL_PYTHON 不是可用的 CPython 3.9：${explicitPython}。请修正或取消该环境变量。`,
                { cause }
            );
        }
    }

    const attemptedCandidates = [...new Set(candidates)];
    for (const candidate of attemptedCandidates) {
        try {
            await validatePython(candidate, runCommand);
            process.stdout.write(`sidecar 构建使用 Python：${candidate}\n`);
            return candidate;
        } catch {
            // 候选不存在或版本不符时继续检查下一个受控候选。
        }
    }
    throw new Error(
        `未找到可用的 CPython 3.9；已尝试：${attemptedCandidates.join('、')}。`
        + '请安装 CPython 3.9，或设置 AITOOL_PYTHON 为其绝对路径。'
    );
}

/**
 * 校验基础或隔离构建解释器的实现与版本。
 * 流程：用 Python 隔离模式执行最小版本断言，要求实现为 CPython 且主次版本精确等于 3.9。
 * 参数：python 为待校验解释器命令或绝对路径；runCommand 为生产或测试命令执行器。
 * 返回：校验成功时无业务返回值。
 * 异常/边界：PyPy、非 3.9、解释器无法启动或校验进程失败均拒绝当前候选；是否检查下一候选由解释器解析流程决定。
 */
async function validatePython(python, runCommand) {
    await runCommand(python, [
        '-I', '-c',
        'import platform, sys; actual=f"{platform.python_implementation()} {platform.python_version()}"; raise SystemExit(0 if sys.implementation.name == "cpython" and sys.version_info[:2] == (3, 9) else f"sidecar 构建必须使用 CPython 3.9，当前为 {actual}")'
    ]);
}

/**
 * 映射当前平台到 Tauri externalBin 目标三元组。
 * 流程：组合 Node platform 与 arch 后查稳定映射。
 * 参数：无。
 * 返回：Tauri 识别的目标三元组。
 * 异常/边界：未登记平台抛错，禁止生成名称错误且无法打包的产物。
 */
function targetTriple() {
    const key = `${process.platform}-${process.arch}`;
    const triple = {
        'darwin-arm64': 'aarch64-apple-darwin',
        'darwin-x64': 'x86_64-apple-darwin'
    }[key];
    if (!triple) throw new Error(`不支持的 sidecar 构建目标：${key}`);
    return triple;
}

/**
 * 执行构建子命令并继承终端输出。
 * 流程：在项目根目录 spawn，等待 exit 事件。
 * 参数：command 为可执行命令，args 为参数数组。
 * 返回：退出码为 0 时 resolve 的 Promise。
 * 异常/边界：启动失败、signal 中止或非零退出均 reject，不产生伪成功。
 */
function run(command, args) {
    return new Promise((resolvePromise, reject) => {
        const child = spawn(command, args, { cwd: projectRoot, stdio: 'inherit' });
        child.once('error', reject);
        child.once('exit', (code, signal) => {
            if (code === 0) resolvePromise();
            else reject(new Error(`sidecar 构建失败：exit=${code ?? 'null'} signal=${signal ?? 'none'}`));
        });
    });
}

/**
 * 构建不依赖最终用户 Python 的单文件 sidecar。
 * 流程：校验入口、准备锁定环境，在临时目录运行 PyInstaller，复制为目标三元组名称并设执行权限。
 * 参数：无。
 * 返回：成功时无业务返回值。
 * 异常/边界：临时 spec/work/dist 总在 finally 清理，不复制虚拟环境、缓存、配置或密钥。
 */
async function main() {
    if (!(await exists(entryFile))) throw new Error(`缺少 FastAPI sidecar 入口：${entryFile}`);
    const temporaryRoot = await mkdtemp(join(tmpdir(), 'codexman-sidecar-'));
    const executableName = process.platform === 'win32' ? 'codexman-ai-sidecar.exe' : 'codexman-ai-sidecar';
    try {
        const python = await prepareIsolatedPython(temporaryRoot);
        const pyInstallerArguments = [
            '-I', '-m', 'PyInstaller', '--noconfirm', '--clean', '--onefile',
            '--name', 'codexman-ai-sidecar', '--paths', join(projectRoot, 'server'),
            '--collect-submodules', 'app', '--hidden-import', 'app.main',
            '--distpath', join(temporaryRoot, 'dist'), '--workpath', join(temporaryRoot, 'work'),
            '--specpath', join(temporaryRoot, 'spec')
        ];
        if (process.platform === 'darwin' && process.env.AITOOL_SIDECAR_SKIP_CODESIGN !== '1') {
            const tauriConfig = JSON.parse(
                await readFile(join(projectRoot, 'src-tauri', 'tauri.conf.json'), 'utf8')
            );
            const signingIdentity = tauriConfig.bundle?.macOS?.signingIdentity?.trim();
            if (signingIdentity) {
                // PyInstaller 必须先用 App 的 Team ID 签内部 Framework，否则 Hardened Runtime 会拒绝 onefile 解压产物。
                pyInstallerArguments.push('--codesign-identity', signingIdentity);
            }
        }
        pyInstallerArguments.push(entryFile);
        await run(python, pyInstallerArguments);
        const extension = process.platform === 'win32' ? '.exe' : '';
        const outputPath = join(binaryDirectory, `codexman-ai-sidecar-${targetTriple()}${extension}`);
        await mkdir(binaryDirectory, { recursive: true });
        await cp(join(temporaryRoot, 'dist', executableName), outputPath);
        if (process.platform !== 'win32') await chmod(outputPath, 0o755);
        process.stdout.write(`sidecar 已生成：${outputPath}\n`);
    } finally {
        await rm(temporaryRoot, { recursive: true, force: true });
    }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    await main();
}
