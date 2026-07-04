#!/usr/bin/env node
'use strict';

console.log = (...args) => console.error(...args);

const readline = require('node:readline');
const {
  createRtmClient,
  RtmConfig,
  RtmLogConfig,
  RtmLogLevel,
} = require('rtm_nodejs');

let client = null;

function emit(value) {
  process.stdout.write(JSON.stringify(value) + '\n');
}

function requireParam(params, name) {
  const value = params && params[name];
  if (value === undefined || value === null || value === '') {
    throw new Error(`missing param ${name}`);
  }
  return String(value);
}

async function connect(params) {
  if (client) {
    await logout();
  }

  const appId = requireParam(params, 'appId');
  const uid = requireParam(params, 'uid');
  const token = requireParam(params, 'token');

  client = await createRtmClient(new RtmConfig({
    appId,
    userId: uid,
    useStringUserId: true,
    logConfig: new RtmLogConfig({
      filePath: '/tmp/enabot_native_rtm_sidecar.log',
      fileSizeKb: 1024,
      logLevel: RtmLogLevel.RTM_LOG_LEVEL_INFO,
    }),
  }));

  if (!client.isValid) {
    throw new Error(`create failed: ${client.getErrorReason(client.createResult)} (${client.createResult})`);
  }

  client.on('message', (event) => {
    emit({
      type: 'event',
      event: 'message',
      publisher: event.publisher,
      channelName: event.channelName,
      message: String(event.message || ''),
    });
  });

  client.on('linkState', (event) => {
    emit({
      type: 'event',
      event: 'linkState',
      previousState: event.previousState,
      currentState: event.currentState,
      reason: event.reason,
    });
  });

  await client.login(token);
  return { connected: true, uid };
}

async function sendUserMessage(params) {
  if (!client) throw new Error('not connected');
  const peerId = requireParam(params, 'peerId');
  const message = requireParam(params, 'message');
  await client.sendUserMessage(peerId, message);
  return { sent: true, peerId };
}

async function logout() {
  if (!client) return { loggedOut: true };
  const old = client;
  client = null;
  try {
    await old.logout();
  } finally {
    old.release();
  }
  return { loggedOut: true };
}

async function dispatch(request) {
  const method = request.method;
  if (method === 'connect') return connect(request.params || {});
  if (method === 'send_user_message') return sendUserMessage(request.params || {});
  if (method === 'logout') return logout();
  throw new Error(`unknown method ${method}`);
}

const rl = readline.createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});

rl.on('line', async (line) => {
  let request = null;
  try {
    request = JSON.parse(line);
    const result = await dispatch(request);
    emit({ type: 'response', id: request.id, ok: true, result });
  } catch (err) {
    emit({
      type: 'response',
      id: request && request.id,
      ok: false,
      error: err && err.stack ? err.stack : String(err),
    });
  }
});

process.on('SIGINT', async () => {
  try { await logout(); } finally { process.exit(0); }
});

process.on('SIGTERM', async () => {
  try { await logout(); } finally { process.exit(0); }
});
