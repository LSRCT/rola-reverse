#!/usr/bin/env node
'use strict';

const fs = require('fs');
const http = require('http');
const path = require('path');

const ROOT = path.resolve(__dirname, '..', '..');
const HARNESS = path.join(__dirname, 'rola_rtm_harness.html');
const SDK_CANDIDATES = [
  path.join(ROOT, 'artifacts', 'agora-node-test', 'node_modules', 'agora-rtm-sdk', 'index.js'),
  path.join(ROOT, 'artifacts', 'agora-node-test-151', 'node_modules', 'agora-rtm-sdk', 'index.js'),
];

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

function sdkPath() {
  for (const candidate of SDK_CANDIDATES) {
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://127.0.0.1');
  if (url.pathname === '/' || url.pathname === '/index.html') {
    serveFile(res, HARNESS, 'text/html; charset=utf-8');
    return;
  }
  if (url.pathname === '/agora-rtm-sdk.js') {
    const found = sdkPath();
    if (!found) {
      res.writeHead(500, { 'Content-Type': 'text/plain; charset=utf-8' });
      res.end('agora-rtm-sdk not installed under artifacts/agora-node-test');
      return;
    }
    serveFile(res, found, 'application/javascript; charset=utf-8');
    return;
  }
  res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
  res.end('not found');
});

const port = Number(process.env.PORT || process.argv[2] || 18765);
server.listen(port, '127.0.0.1', () => {
  console.log(`ROLA RTM harness listening on http://127.0.0.1:${port}/`);
});
