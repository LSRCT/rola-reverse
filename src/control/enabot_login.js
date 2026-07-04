#!/usr/bin/env node
'use strict';

const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '..', '..');
const DEFAULT_OUT = path.join(ROOT, 'artifacts', 'captures', 'enabot_login_result.json');
const HOST = 'ebo.enabotserverintl.com';
const LOGIN_PATH = '/api/v1/s1/users/login/';
const LOGIN_URL = `https://${HOST}${LOGIN_PATH}`;
const ALPHABET = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';

function need(name) {
  const value = process.env[name];
  if (!value) {
    throw new Error(`missing required env ${name}`);
  }
  return value;
}

function optional(name, fallback = '') {
  return process.env[name] || fallback;
}

function javaUrlEncode(value) {
  return encodeURIComponent(String(value))
    .replace(/[!'()]/g, ch => `%${ch.charCodeAt(0).toString(16).toUpperCase()}`)
    .replace(/\*/g, '%2A')
    .replace(/%7E/g, '~');
}

function randomNonce(length = 11) {
  let out = '';
  for (let i = 0; i < length; i += 1) {
    out += ALPHABET[crypto.randomInt(0, ALPHABET.length)];
  }
  return out;
}

function canonicalParams(params) {
  return Object.keys(params).sort().map(key => {
    const value = params[key];
    const encodedValue = value !== null && typeof value === 'object'
      ? JSON.stringify(value)
      : String(value);
    return `${javaUrlEncode(key)}=${javaUrlEncode(encodedValue)}`;
  }).join('&');
}

function sign(method, relativePath, params, secret) {
  const canonical = canonicalParams(params);
  const signText = [
    method.toUpperCase(),
    javaUrlEncode(relativePath),
    javaUrlEncode(canonical),
  ].join('&');
  return crypto.createHmac('sha1', Buffer.from(secret, 'utf8'))
    .update(Buffer.from(signText, 'utf8'))
    .digest('base64');
}

function encryptBody(body, key) {
  const keyBuf = Buffer.from(key, 'utf8');
  if (keyBuf.length !== 16) {
    throw new Error(`ENABOT_BODY_ENCRYPT_KEY_S2 must be 16 bytes, got ${keyBuf.length}`);
  }

  const iv = crypto.randomBytes(16);
  const cipher = crypto.createCipheriv('aes-128-gcm', keyBuf, iv, { authTagLength: 16 });
  const encrypted = Buffer.concat([
    cipher.update(Buffer.from(JSON.stringify(body), 'utf8')),
    cipher.final(),
  ]);
  return Buffer.concat([iv, encrypted, cipher.getAuthTag()]).toString('base64');
}

function buildEnvelope(body, relativePath) {
  const envelope = {
    data: encryptBody(body, need('ENABOT_BODY_ENCRYPT_KEY_S2')),
    app_type: 2,
    e_ver: '1.0',
    nonce: randomNonce(),
    timestamp: Number(optional('ENABOT_TIMESTAMP', String(Date.now()))),
    signature_version: '1.1',
    signature_method: 'SHA1',
  };
  envelope.sign = sign('POST', relativePath, envelope, need('ENABOT_ACCESS_KEY_SECRET_S2'));
  return envelope;
}

function getSetCookies(headers) {
  if (typeof headers.getSetCookie === 'function') {
    return headers.getSetCookie();
  }
  const one = headers.get('set-cookie');
  return one ? [one] : [];
}

function cookieNames(setCookies) {
  return setCookies.map(cookie => cookie.split('=', 1)[0]).filter(Boolean);
}

async function main() {
  const account = need('ENABOT_ACCOUNT');
  const password = need('ENABOT_PASSWORD');
  const loginRegion = optional('ENABOT_LOGIN_REGION', 'GB');
  const phoneArea = optional('ENABOT_PHONE_AREA', '');

  const body = {
    app_kind: 'Android',
    app_token: need('ENABOT_APP_TOKEN'),
    device_id: need('ENABOT_DEVICE_ID'),
    language: optional('ENABOT_LANGUAGE', 'en'),
    account,
    password,
    login_region: loginRegion,
  };
  if (phoneArea) body.phone_area = phoneArea;

  const envelope = buildEnvelope(body, LOGIN_PATH);
  const response = await fetch(LOGIN_URL, {
    method: 'POST',
    headers: {
      'Accept-Language': optional('ENABOT_ACCEPT_LANGUAGE', 'en'),
      'Content-Type': 'application/json; charset=utf-8',
    },
    body: JSON.stringify(envelope),
  });

  const text = await response.text();
  const setCookies = getSetCookies(response.headers);
  let parsed = null;
  try { parsed = JSON.parse(text); } catch (_) {}

  const output = {
    ts: new Date().toISOString(),
    httpStatus: response.status,
    ok: response.ok,
    cookieNames: cookieNames(setCookies),
    setCookies,
    rawBody: text,
    parsed,
  };

  const outPath = process.argv[2] || DEFAULT_OUT;
  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.writeFileSync(outPath, JSON.stringify(output, null, 2));

  const summary = {
    httpStatus: response.status,
    apiCode: parsed && parsed.code,
    apiMsg: parsed && parsed.msg,
    cookieNames: output.cookieNames,
    saved: outPath,
  };
  console.log(JSON.stringify(summary, null, 2));
}

main().catch(err => {
  console.error(err && err.stack ? err.stack : String(err));
  process.exit(1);
});
