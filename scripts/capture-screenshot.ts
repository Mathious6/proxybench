import { mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(scriptDirectory, "..");
const port = 1430;
const url = `http://127.0.0.1:${port}/?screenshot=1`;
const output = resolve(projectRoot, "docs/assets/screenshot.png");
const playwrightCli = resolve(projectRoot, "node_modules/playwright/cli.js");
const viteCli = resolve(projectRoot, "node_modules/vite/bin/vite.js");

let server: Bun.Subprocess | undefined;
let browser: Awaited<ReturnType<typeof chromium.launch>> | undefined;
let stopping: Promise<void> | undefined;

async function waitForServer() {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        return;
      }
    } catch {}
    await Bun.sleep(100);
  }
  throw new Error(`Vite did not become ready at ${url}`);
}

async function run(command: string[]) {
  const process = Bun.spawn(command, { cwd: projectRoot, stdout: "inherit", stderr: "inherit" });
  if ((await process.exited) !== 0) {
    throw new Error(`Command failed: ${command.join(" ")}`);
  }
}

async function stopServer() {
  if (!server || server.exitCode !== null) {
    return;
  }
  server.kill("SIGTERM");
  await Promise.race([server.exited.then(() => {}), Bun.sleep(5_000)]);
  if (server.exitCode === null) {
    server.kill("SIGKILL");
    await server.exited;
  }
}

function cleanup() {
  stopping ??= (async () => {
    await browser?.close();
    await stopServer();
  })();
  return stopping;
}

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.once(signal, () => {
    void cleanup().finally(() => process.exit(128));
  });
}

try {
  await run([process.execPath, playwrightCli, "install", "chromium"]);
  server = Bun.spawn(
    [process.execPath, viteCli, "dev", "--host", "127.0.0.1", "--port", String(port), "--strictPort"],
    { cwd: projectRoot, stdout: "inherit", stderr: "inherit" },
  );
  await waitForServer();
  browser = await chromium.launch();
  const context = await browser.newContext({
    viewport: { width: 1120, height: 656 },
    deviceScaleFactor: 2,
    locale: "en-US",
    timezoneId: "UTC",
  });
  const page = await context.newPage();
  await page.goto(url, { waitUntil: "domcontentloaded" });
  await page.waitForSelector('html[data-screenshot-ready="true"]');
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  await page.addStyleTag({
    content: "*, *::before, *::after { animation: none !important; caret-color: transparent !important; transition: none !important; }",
  });
  await mkdir(dirname(output), { recursive: true });
  await page.screenshot({ path: output, animations: "disabled" });
  await context.close();
} finally {
  await cleanup();
}
