import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const ports = process.argv.slice(2).length
  ? process.argv.slice(2).map((value) => Number(value))
  : [1420];

if (ports.some((port) => !Number.isInteger(port) || port <= 0)) {
  process.stderr.write("请传入有效的本地端口号。\n");
  process.exit(1);
}

for (const port of ports) {
  await freePort(port);
}

/** 释放指定本地端口上的旧监听进程，避免 Vite 启动时因为残留进程失败。 */
async function freePort(targetPort) {
  const processIds = await findListenProcessIds(targetPort);
  if (!processIds.length) {
    return;
  }

  process.stdout.write(`释放已占用的本地端口 ${targetPort}：${processIds.join(", ")}\n`);
  await Promise.all(processIds.map((processId) => killProcess(processId)));
  await waitPortFree(targetPort);
}

/** 查询正在监听指定 TCP 端口的进程 ID 列表。 */
async function findListenProcessIds(targetPort) {
  try {
    const { stdout } = await execFileAsync("lsof", [
      "-tiTCP:" + targetPort,
      "-sTCP:LISTEN",
    ]);
    return stdout
      .split(/\s+/)
      .map((value) => Number(value))
      .filter((value) => Number.isInteger(value) && value > 0 && value !== process.pid);
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && error.code === 1) {
      return [];
    }
    throw error;
  }
}

/** 结束指定进程；如果进程已经退出，则视为端口清理成功的一部分。 */
async function killProcess(processId) {
  try {
    process.kill(processId, "SIGTERM");
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && error.code === "ESRCH") {
      return;
    }
    throw error;
  }
}

/** 等待端口释放；超过短暂等待后使用 SIGKILL 兜底清理残留监听。 */
async function waitPortFree(targetPort) {
  for (let index = 0; index < 20; index += 1) {
    await sleep(100);
    const processIds = await findListenProcessIds(targetPort);
    if (!processIds.length) {
      return;
    }
    if (index === 9) {
      processIds.forEach((processId) => {
        try {
          process.kill(processId, "SIGKILL");
        } catch {
          // 进程可能在检查和兜底清理之间已经退出。
        }
      });
    }
  }
  throw new Error(`端口 ${targetPort} 仍被占用，请检查残留进程。`);
}

/** 等待指定毫秒数。 */
function sleep(milliseconds) {
  return new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}
