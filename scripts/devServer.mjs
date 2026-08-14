import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { networkInterfaces } from "node:os";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const rootDir = normalize(fileURLToPath(new URL("..", import.meta.url)));
const distDir = join(rootDir, "dist");
const port = Number(process.env.PORT || 1421);
const host = process.env.HOST || "0.0.0.0";

const mimeByExt = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
};

const server = createServer(async (req, res) => {
  try {
    const url = new URL(req.url || "/", `http://${req.headers.host || "localhost"}`);
    await serveStatic(url.pathname, res);
  } catch (error) {
    writeJson(res, 500, { error: formatError(error) });
  }
});

server.listen(port, host, () => {
  const urls = [`http://127.0.0.1:${port}`, ...listLanUrls(port)];
  process.stdout.write(`CodexMan 网页预览：\n${urls.map((url) => `  ${url}`).join("\n")}\n`);
});

/** 获取当前机器可用于局域网访问的 IPv4 地址。 */
function listLanUrls(currentPort) {
  return Object.values(networkInterfaces())
    .flatMap((interfaces) => interfaces || [])
    .filter((item) => item.family === "IPv4" && !item.internal)
    .map((item) => `http://${item.address}:${currentPort}`);
}

/** 提供构建后的前端静态资源。 */
async function serveStatic(pathname, res) {
  const safePath = pathname === "/" ? "/index.html" : pathname;
  const filePath = normalize(join(distDir, safePath));
  if (!filePath.startsWith(distDir)) {
    writeJson(res, 403, { error: "Forbidden" });
    return;
  }
  try {
    const content = await readFile(filePath);
    res.writeHead(200, {
      "Content-Type": mimeByExt[extname(filePath)] || "application/octet-stream",
      "Cache-Control": "no-store",
    });
    res.end(content);
  } catch {
    if (safePath !== "/index.html") {
      await serveStatic("/", res);
      return;
    }
    writeJson(res, 404, { error: "请先执行 npm run build" });
  }
}

/** 写入 JSON 响应。 */
function writeJson(res, statusCode, payload) {
  res.writeHead(statusCode, {
    "Content-Type": "application/json; charset=utf-8",
    "Cache-Control": "no-store",
  });
  res.end(JSON.stringify(payload));
}

/** 格式化错误信息。 */
function formatError(error) {
  return error instanceof Error ? error.message : String(error);
}
