import assert from 'node:assert/strict';
import { chmod, mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import process from 'node:process';
import test from 'node:test';

import { prepareIsolatedPython, resolveBuildPython } from './buildSidecar.mjs';

/**
 * 在测试临时目录生成可读取的隔离 Python 占位文件。
 * 流程：按当前平台计算 venv Python 路径，创建父目录并写入空文件；非 Windows 平台补充执行权限以贴近真实 venv 产物。
 * 参数：environmentDirectory 为 prepareIsolatedPython 传给 venv 的目标目录。
 * 返回：生成的隔离 Python 绝对路径。
 * 异常/边界：文件系统创建失败直接抛出，由 node:test 标记用例失败；不接触项目内真实构建目录。
 */
async function createPythonPlaceholder(environmentDirectory) {
    const python = process.platform === 'win32'
        ? join(environmentDirectory, 'Scripts', 'python.exe')
        : join(environmentDirectory, 'bin', 'python');
    await mkdir(dirname(python), { recursive: true });
    await writeFile(python, '');
    if (process.platform !== 'win32') await chmod(python, 0o755);
    return python;
}

test('SIDECAR-BUILD-001 显式解释器只用于校验和派生全新隔离 venv', /**
 * 验证显式基础解释器无法把自身环境污染带入 Sidecar 构建。
 * 流程：注入记录命令的执行器，断言基础解释器只执行版本校验和 venv 创建，而版本复核与带哈希依赖安装全部由临时 venv 解释器执行。
 * 参数：无，测试数据和命令执行器均在用例内部创建。
 * 返回：所有命令顺序与安全参数断言通过后完成 Promise。
 * 异常/边界：任一命令来源或隔离参数不符合预期即使测试失败；无论成功失败都恢复 AITOOL_PYTHON 并删除测试临时目录。
 */ async () => {
    const temporaryRoot = await mkdtemp(join(tmpdir(), 'codexman-sidecar-build-test-'));
    const previousPython = process.env.AITOOL_PYTHON;
    const commands = [];
    process.env.AITOOL_PYTHON = '/custom/cpython3.9';
    try {
        const isolatedPython = await createPythonPlaceholder(join(temporaryRoot, 'build-environment'));
        const result = await prepareIsolatedPython(temporaryRoot, async (command, args) => {
            commands.push({ command, args });
        });

        assert.equal(result, isolatedPython);
        assert.equal(commands.length, 4);
        assert.deepEqual(commands[0], {
            command: '/custom/cpython3.9',
            args: ['-I', '-c', commands[0].args[2]]
        });
        assert.deepEqual(commands[1], {
            command: '/custom/cpython3.9',
            args: ['-I', '-m', 'venv', join(temporaryRoot, 'build-environment')]
        });
        assert.equal(commands[2].command, isolatedPython);
        assert.deepEqual(commands[2].args.slice(0, 2), ['-I', '-c']);
        assert.equal(commands[3].command, isolatedPython);
        assert.deepEqual(commands[3].args.slice(0, 6), [
            '-I', '-m', 'pip', 'install', '--isolated', '--disable-pip-version-check'
        ]);
        assert.ok(commands[3].args.includes('--require-hashes'));
        assert.ok(commands[3].args.includes('--only-binary=:all:'));
        assert.ok(!commands.some(({ command, args }) => (
            command === '/custom/cpython3.9' && args.includes('pip')
        )));
        assert.ok(!commands.some(({ command, args }) => (
            command === '/custom/cpython3.9' && args.includes('PyInstaller')
        )));
    } finally {
        if (previousPython === undefined) delete process.env.AITOOL_PYTHON;
        else process.env.AITOOL_PYTHON = previousPython;
        await rm(temporaryRoot, { recursive: true, force: true });
    }
});

test('SIDECAR-BUILD-002 venv 未生成解释器时失败且不会安装依赖', /**
 * 验证隔离 venv 未生成 Python 时构建供应链门禁会关闭。
 * 流程：注入不生成文件的命令执行器，执行解释器准备并断言其在基础校验和 venv 命令后抛错，且不会继续调用 pip。
 * 参数：无，临时目录、环境变量快照和命令记录均由用例内部维护。
 * 返回：预期异常、命令序列与无 pip 调用断言全部通过后完成 Promise。
 * 异常/边界：如果错误未抛出、错误信息不稳定或依赖安装被触发则测试失败；无论成功失败都恢复环境变量并清理临时目录。
 */ async () => {
    const temporaryRoot = await mkdtemp(join(tmpdir(), 'codexman-sidecar-build-test-'));
    const previousPython = process.env.AITOOL_PYTHON;
    const commands = [];
    delete process.env.AITOOL_PYTHON;
    try {
        await assert.rejects(
            prepareIsolatedPython(temporaryRoot, async (command, args) => {
                commands.push({ command, args });
            }),
            /隔离 sidecar 构建环境未生成 Python/
        );
        const expectedPython = process.platform === 'darwin' ? '/usr/bin/python3' : 'python3.9';
        assert.deepEqual(commands.map(({ command, args }) => [command, ...args.slice(0, 3)]), [
            [expectedPython, '-I', '-c', commands[0].args[2]],
            [expectedPython, '-I', '-m', 'venv']
        ]);
        assert.ok(!commands.some(({ args }) => args.includes('pip')));
    } finally {
        if (previousPython === undefined) delete process.env.AITOOL_PYTHON;
        else process.env.AITOOL_PYTHON = previousPython;
        await rm(temporaryRoot, { recursive: true, force: true });
    }
});

test('SIDECAR-BUILD-003 自动跳过不合规 PATH Python 并选择 CPython 3.9', /**
 * 验证用户 Shell 中 python3 指向新版本时仍能稳定找到受控的 3.9 候选。
 * 流程：注入两个候选及命令执行器，让首个候选校验失败、第二个成功，断言选择结果和探测顺序。
 * 参数：无，候选与命令执行器均在用例内部注入。
 * 返回：回退选择符合预期后完成 Promise。
 * 异常/边界：未跳过错误版本、顺序漂移或继续探测第三个候选均使测试失败。
 */ async () => {
    const previousPython = process.env.AITOOL_PYTHON;
    const commands = [];
    delete process.env.AITOOL_PYTHON;
    try {
        const python = await resolveBuildPython(async (command, args) => {
            commands.push({ command, args });
            if (command === '/path/python3.14') throw new Error('Python 3.14');
        }, ['/path/python3.14', '/usr/bin/python3']);

        assert.equal(python, '/usr/bin/python3');
        assert.deepEqual(commands.map(({ command }) => command), [
            '/path/python3.14',
            '/usr/bin/python3'
        ]);
        assert.ok(commands.every(({ args }) => args.slice(0, 2).join(' ') === '-I -c'));
    } finally {
        if (previousPython === undefined) delete process.env.AITOOL_PYTHON;
        else process.env.AITOOL_PYTHON = previousPython;
    }
});

test('SIDECAR-BUILD-004 显式解释器错误时禁止静默回退', /**
 * 验证显式供应链配置错误会直接暴露，避免悄悄改用另一解释器生成不可复现产物。
 * 流程：设置 AITOOL_PYTHON 为错误版本，注入必定失败的执行器并断言稳定错误信息和单次校验。
 * 参数：无，环境变量与执行器均在用例内部维护。
 * 返回：预期异常与无回退断言通过后完成 Promise。
 * 异常/边界：如果继续检查自动候选或错误信息不含显式路径则测试失败。
 */ async () => {
    const previousPython = process.env.AITOOL_PYTHON;
    const commands = [];
    process.env.AITOOL_PYTHON = '/custom/python3.14';
    try {
        await assert.rejects(
            resolveBuildPython(async (command, args) => {
                commands.push({ command, args });
                throw new Error('Python 3.14');
            }, ['/usr/bin/python3']),
            /AITOOL_PYTHON 不是可用的 CPython 3\.9：\/custom\/python3\.14/
        );
        assert.deepEqual(commands.map(({ command }) => command), ['/custom/python3.14']);
    } finally {
        if (previousPython === undefined) delete process.env.AITOOL_PYTHON;
        else process.env.AITOOL_PYTHON = previousPython;
    }
});

test('SIDECAR-BUILD-005 全部候选失败时返回可操作诊断', /**
 * 验证机器未安装 CPython 3.9 时错误会说明探测范围和修复入口。
 * 流程：注入两个均失败的候选，断言错误包含完整候选清单与 AITOOL_PYTHON 指引。
 * 参数：无，候选与执行器均在用例内部注入。
 * 返回：错误内容和探测次数符合预期后完成 Promise。
 * 异常/边界：候选遗漏、重复探测或错误缺少修复方法均使测试失败。
 */ async () => {
    const previousPython = process.env.AITOOL_PYTHON;
    const commands = [];
    delete process.env.AITOOL_PYTHON;
    try {
        await assert.rejects(
            resolveBuildPython(async (command) => {
                commands.push(command);
                throw new Error('not compatible');
            }, ['python3.14', 'python3.9', 'python3.14']),
            /已尝试：python3\.14、python3\.9.*AITOOL_PYTHON/
        );
        assert.deepEqual(commands, ['python3.14', 'python3.9']);
    } finally {
        if (previousPython === undefined) delete process.env.AITOOL_PYTHON;
        else process.env.AITOOL_PYTHON = previousPython;
    }
});

test('SIDECAR-BUILD-006 显式空白解释器配置立即失败', /**
 * 验证空白 AITOOL_PYTHON 不会被误判为未设置后自动回退。
 * 流程：设置仅含空格的环境变量，注入禁止执行的命令执行器并断言绝对路径错误。
 * 参数：无，环境变量和执行器均在用例内部维护。
 * 返回：稳定错误与零命令调用断言通过后完成 Promise。
 * 异常/边界：如果启动任何候选解释器或进入自动探测则测试失败。
 */ async () => {
    const previousPython = process.env.AITOOL_PYTHON;
    let commandCount = 0;
    process.env.AITOOL_PYTHON = '   ';
    try {
        await assert.rejects(
            resolveBuildPython(async () => {
                commandCount += 1;
            }, ['/usr/bin/python3']),
            /AITOOL_PYTHON 必须是.*非空绝对路径/
        );
        assert.equal(commandCount, 0);
    } finally {
        if (previousPython === undefined) delete process.env.AITOOL_PYTHON;
        else process.env.AITOOL_PYTHON = previousPython;
    }
});

test('SIDECAR-BUILD-007 显式相对解释器命令立即失败', /**
 * 验证 AITOOL_PYTHON 不能通过 PATH 间接解析，保证显式构建来源确定。
 * 流程：设置相对命令名，注入禁止执行的命令执行器并断言绝对路径错误。
 * 参数：无，环境变量和执行器均在用例内部维护。
 * 返回：稳定错误与零命令调用断言通过后完成 Promise。
 * 异常/边界：如果接受相对命令或继续自动候选探测则测试失败。
 */ async () => {
    const previousPython = process.env.AITOOL_PYTHON;
    let commandCount = 0;
    process.env.AITOOL_PYTHON = 'python3.9';
    try {
        await assert.rejects(
            resolveBuildPython(async () => {
                commandCount += 1;
            }, ['/usr/bin/python3']),
            /AITOOL_PYTHON 必须是.*非空绝对路径/
        );
        assert.equal(commandCount, 0);
    } finally {
        if (previousPython === undefined) delete process.env.AITOOL_PYTHON;
        else process.env.AITOOL_PYTHON = previousPython;
    }
});
