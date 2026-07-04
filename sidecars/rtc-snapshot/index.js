#!/usr/bin/env node
'use strict';

console.log = (...args) => console.error(...args);

const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const { chromium } = require('playwright-core');

const ROOT = path.resolve(__dirname, '..', '..');
const PAGE = path.join(__dirname, 'capture.html');
const AGORA_SDK = path.join(
  __dirname,
  'node_modules',
  'agora-rtc-sdk-ng',
  'AgoraRTC_N-production.js',
);

function readStdin() {
  return new Promise((resolve, reject) => {
    let input = '';
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', chunk => { input += chunk; });
    process.stdin.on('error', reject);
    process.stdin.on('end', () => {
      try {
        resolve(JSON.parse(input));
      } catch (err) {
        reject(new Error(`invalid JSON input: ${err.message || err}`));
      }
    });
  });
}

function need(cfg, name) {
  const value = cfg[name];
  if (value === undefined || value === null || value === '') {
    throw new Error(`missing config field ${name}`);
  }
  return value;
}

function serveFile(res, filePath, type) {
  fs.readFile(filePath, (err, data) => {
    if (err) {
      res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
      res.end(String(err.message || err));
      return;
    }
    res.writeHead(200, {
      'Content-Type': type,
      'Cache-Control': 'no-store',
    });
    res.end(data);
  });
}

function createServer() {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, 'http://127.0.0.1');
    if (url.pathname === '/' || url.pathname === '/capture.html') {
      serveFile(res, PAGE, 'text/html; charset=utf-8');
      return;
    }
    if (url.pathname === '/agora-rtc-sdk-ng.js') {
      serveFile(res, AGORA_SDK, 'application/javascript; charset=utf-8');
      return;
    }
    if (url.pathname === '/favicon.ico') {
      res.writeHead(204, { 'Cache-Control': 'no-store' });
      res.end();
      return;
    }
    res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end('not found');
  });

  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      resolve({
        server,
        port: server.address().port,
      });
    });
  });
}

function encodeConfig(cfg) {
  return Buffer.from(JSON.stringify(cfg), 'utf8')
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/g, '');
}

function chromeExecutable() {
  const candidates = [
    process.env.CHROME_PATH,
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
  ].filter(Boolean);
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) return candidate;
  }
  throw new Error('no Chrome-compatible executable found; set CHROME_PATH');
}

function outputPath(raw) {
  const requested = String(raw || 'artifacts/snapshots/latest.jpg');
  return path.isAbsolute(requested) ? requested : path.join(ROOT, requested);
}

function writeImage(outPath, dataUrl) {
  const match = /^data:image\/(png|jpeg);base64,(.+)$/i.exec(dataUrl || '');
  if (!match) {
    throw new Error('capture result did not include a PNG/JPEG data URL');
  }
  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.writeFileSync(outPath, Buffer.from(match[2], 'base64'));
}

async function main() {
  const cfg = await readStdin();
  const outPath = outputPath(cfg.out);
  const waitMs = Number(cfg.waitMs || 30_000);

  need(cfg, 'appId');
  need(cfg, 'uid');
  need(cfg, 'token');
  need(cfg, 'channel');

  const { server, port } = await createServer();
  let browser = null;
  try {
    browser = await chromium.launch({
      executablePath: chromeExecutable(),
      headless: true,
      args: [
        '--autoplay-policy=no-user-gesture-required',
        '--use-fake-ui-for-media-stream',
      ],
    });

    const page = await browser.newPage({
      viewport: { width: 1280, height: 720 },
    });
    page.on('console', msg => {
      const text = msg.text();
      if (/^(CONFIG|CONNECTION_STATE|JOIN_OK|USER_|SUBSCRIBE_|CAPTURE_|ERROR|LEAVE_)/.test(text)) {
        console.error(`[rtc-page:${msg.type()}] ${text}`);
      }
    });
    page.on('pageerror', err => {
      console.error(`[rtc-page:error] ${err.stack || err.message || err}`);
    });

    const hash = encodeConfig({
      appId: String(cfg.appId),
      uid: String(cfg.uid),
      token: String(cfg.token),
      channel: String(cfg.channel),
      expectedPublisher: cfg.expectedPublisher === undefined
        ? ''
        : String(cfg.expectedPublisher),
      mode: String(cfg.mode || 'rtc'),
      codec: String(cfg.codec || 'h264'),
      waitMs,
      stabilizeMs: Number(cfg.stabilizeMs || 1200),
    });
    await page.goto(`http://127.0.0.1:${port}/capture.html#${hash}`, {
      waitUntil: 'domcontentloaded',
      timeout: 10_000,
    });

    await page.waitForFunction(
      () => window.__rtcSnapshot && window.__rtcSnapshot.done,
      null,
      { timeout: waitMs + 10_000 },
    );
    const result = await page.evaluate(() => window.__rtcSnapshot);
    if (!result || !result.ok) {
      throw new Error((result && result.error) || 'RTC snapshot capture failed');
    }

    writeImage(outPath, result.dataUrl);
    delete result.dataUrl;
    result.out = outPath;
    result.bytes = fs.statSync(outPath).size;
    process.stdout.write(JSON.stringify(result, null, 2) + '\n');
  } finally {
    if (browser) await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
}

main().catch(err => {
  console.error(err && err.stack ? err.stack : String(err));
  process.exit(1);
});
