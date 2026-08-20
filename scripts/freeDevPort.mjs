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

for (const port of ports) await assertPortAvailable(port);

/**
 * 确认指定开发端口可用。
 * 流程：查询监听进程；无人占用则继续，有占用则打印 PID 并明确失败。
 * 参数：targetPort 为待使用的 TCP 端口。
 * 返回：端口可用时完成 Promise。
 * 异常/边界：绝不结束未知进程，也不自动漂移端口，避免前后端地址不一致或误杀用户服务。
 */
async function assertPortAvailable(targetPort) {
  const processIds = await findListenProcessIds(targetPort);
  if (!processIds.length) return;
  throw new Error(`开发端口 ${targetPort} 已被进程 ${processIds.join(", ")} 占用，请先确认并手动处理。`);
}

/**
 * 查询正在监听指定 TCP 端口的进程 ID 列表。
 * 流程：通过 lsof 精确查询 LISTEN 状态，解析并过滤无效 PID 与当前进程。
 * 参数：targetPort 为待检查的 TCP 端口。
 * 返回：监听该端口的有效进程 ID 数组；没有监听者时返回空数组。
 * 异常/边界：lsof 状态码 1 表示无匹配并按空数组处理，其它执行错误继续抛出，避免误判端口可用。
 */
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
