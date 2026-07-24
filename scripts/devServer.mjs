import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const rootDir = normalize(fileURLToPath(new URL("..", import.meta.url)));
const distDir = join(rootDir, "dist");
const port = Number(process.env.PORT || 1421);
const maxJsonBytes = 32 * 1024 * 1024;

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
    if (req.method === "POST" && url.pathname === "/api/transcribe") {
      await handleTranscribe(req, res);
      return;
    }
    await serveStatic(url.pathname, res);
  } catch (error) {
    writeJson(res, 500, { error: formatError(error) });
  }
});

server.listen(port, () => {
  process.stdout.write(`typesass 网页预览：http://127.0.0.1:${port}\n`);
});

/** 处理本地网页预览模式下的转写请求。 */
async function handleTranscribe(req, res) {
  const payload = await readJson(req);
  const apiKey = readString(payload.apiKey) || process.env.MIMO_API_KEY || "";
  const baseUrl = trimTrailingSlash(readString(payload.baseUrl) || "https://token-plan-cn.xiaomimimo.com/v1");
  const asrModel = readString(payload.asrModel) || "mimo-v2.5-asr";
  const language = readString(payload.language) || "auto";
  const contentType = readString(payload.contentType) || "audio/webm";
  const audioBase64 = readString(payload.audioBase64);

  if (!apiKey) {
    writeJson(res, 400, { error: "请先填写 Mimo API Key" });
    return;
  }
  if (!audioBase64) {
    writeJson(res, 400, { error: "音频为空" });
    return;
  }

  const startedAt = Date.now();
  const body = {
    model: asrModel,
    messages: [
      {
        role: "user",
        content: [
          {
            type: "input_audio",
            input_audio: {
              data: `data:${contentType};base64,${audioBase64}`,
            },
          },
        ],
      },
    ],
  };
  if (language !== "auto") {
    body.asr_options = { language };
  }

  const mimoResponse = await fetch(`${baseUrl}/chat/completions`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${apiKey}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(60000),
  });
  const responseText = await mimoResponse.text();
  const data = parseJson(responseText);
  if (!mimoResponse.ok) {
    const message = readString(data?.error?.message) || responseText.slice(0, 500) || `HTTP ${mimoResponse.status}`;
    writeJson(res, mimoResponse.status, { error: `Mimo 请求失败：${message}` });
    return;
  }

  writeJson(res, 200, {
    text: readString(data?.choices?.[0]?.message?.content),
    elapsedMs: Date.now() - startedAt,
    model: readString(data?.model) || asrModel,
  });
}

/** 读取 JSON 请求体。 */
async function readJson(req) {
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > maxJsonBytes) {
      throw new Error("请求体过大");
    }
    chunks.push(chunk);
  }
  return parseJson(Buffer.concat(chunks).toString("utf8")) || {};
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

/** 安全解析 JSON。 */
function parseJson(text) {
  try {
    return JSON.parse(text || "{}");
  } catch {
    return null;
  }
}

/** 读取字符串字段。 */
function readString(value) {
  return typeof value === "string" ? value.trim() : "";
}

/** 去掉接口地址末尾斜杠。 */
function trimTrailingSlash(value) {
  return value.replace(/\/+$/, "");
}

/** 格式化错误信息。 */
function formatError(error) {
  return error instanceof Error ? error.message : String(error);
}
