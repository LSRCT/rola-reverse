/*
 * frida_ebo_creds.js — grab the 4 per-device robot secrets from the ROLA app.
 *
 * This is "the app step". Everything else the bridge needs (the 4 TUTK .so
 * libs, the bionic runtime, the app-wide license, the ioctl9930 blob) is
 * app/SDK-level and can be reused from the community per our "same as the
 * community" assumption. What CANNOT be reused is your robot's identity/keys —
 * this script lifts exactly those:
 *
 *     EBO_UID       (20-char Kalay UID)          <- IOTC_Connect_ByUIDEx
 *     EBO_AUTHKEY   (8-char auth key)            <- IOTC_Connect_ByUIDEx input
 *     EBO_IDENTITY  (account UUID)               <- avClientStartEx in-config
 *     EBO_TOKEN     (session token / PSK secret) <- avClientStartEx in-config
 *
 * It also opportunistically dumps the license and the 0x9930 "start streaming"
 * payload, in case yours differ from the community's.
 *
 * Requirements: rooted Android (or emulator) with frida-server running, or a
 * repackaged app with frida-gadget. The app must actually CONNECT to the robot
 * while this is attached (open live view). Close the app on any other phone
 * first — only one client may stream at a time.
 *
 * Run:
 *     frida -U -f com.enabot.rola -l src/extract/frida_ebo_creds.js --no-pause
 *   (replace the package name if different: `frida-ps -Uai | grep -i ebo`)
 *
 * Struct offsets vary by SDK build, so rather than hard-code them this script
 * hexdumps each argument struct and heuristically pulls out the UID / UUID /
 * printable-string fields. Copy the tagged values straight into your .env.
 */

'use strict';

const LIBS = [
  'libIOTCAPIs.so', 'libAVAPIs.so', 'libTUTKGlobalAPIs.so', 'libRDTAPIs.so',
];

const found = {}; // dedupe repeated prints

function log(tag, val) {
  const key = tag + '=' + val;
  if (found[key]) return;
  found[key] = true;
  console.log('\n[[EBO]] ' + tag + ' = ' + JSON.stringify(val));
}

// Read a NUL-terminated C string safely.
function cstr(p) {
  try {
    if (p.isNull()) return '';
    const s = p.readUtf8String();
    return s || '';
  } catch (e) { return ''; }
}

// Pull printable ASCII runs (len>=4) out of a struct blob.
function printableRuns(base, size) {
  const out = [];
  let cur = '';
  for (let i = 0; i < size; i++) {
    let b;
    try { b = base.add(i).readU8(); } catch (e) { break; }
    if (b >= 0x20 && b < 0x7f) { cur += String.fromCharCode(b); }
    else { if (cur.length >= 4) out.push(cur); cur = ''; }
  }
  if (cur.length >= 4) out.push(cur);
  return out;
}

// A Kalay UID looks like 20 chars of [A-Z0-9]; a token/identity often a UUID.
const RE_UID = /^[A-Z0-9]{20}$/;
const RE_UUID = /^[0-9a-fA-F-]{16,40}$/;

function dumpStruct(name, ptr, size) {
  if (ptr.isNull()) return [];
  console.log('  ' + name + ' @ ' + ptr + ' (' + size + 'B):');
  try { console.log(hexdump(ptr, { length: size, ansi: false })); }
  catch (e) {}
  return printableRuns(ptr, size);
}

function attach() {
  const iotc = Module.findExportByName(null, 'IOTC_Connect_ByUIDEx');
  const avStart = Module.findExportByName(null, 'avClientStartEx');
  const setLic = Module.findExportByName(null, 'TUTK_SDK_Set_License_Key') ||
                 Module.findExportByName(null, 'TUTKGlobal_SDK_Set_License_Key');
  const avIoctl = Module.findExportByName(null, 'avSendIOCtrl');

  if (setLic) {
    Interceptor.attach(setLic, {
      onEnter(args) { log('EBO_LICENSE', cstr(args[0])); },
    });
    console.log('[*] hooked SetLicenseKey');
  }

  if (iotc) {
    // int IOTC_Connect_ByUIDEx(const char* UID, St_IOTCConnectInput* in, ...)
    Interceptor.attach(iotc, {
      onEnter(args) {
        const uid = cstr(args[0]);
        if (RE_UID.test(uid)) log('EBO_UID', uid);
        // The input struct (arg1) carries authKey (+ maybe UID again).
        const runs = dumpStruct('St_IOTCConnectInput', args[1], 128);
        runs.forEach(r => {
          if (RE_UID.test(r)) log('EBO_UID', r);
          else if (r.length === 8) log('EBO_AUTHKEY?(8-char)', r);
        });
      },
    });
    console.log('[*] hooked IOTC_Connect_ByUIDEx');
  }

  if (avStart) {
    // avClientStartEx(AVClientStartInConfig* in, AVClientStartOutConfig* out)
    Interceptor.attach(avStart, {
      onEnter(args) {
        const runs = dumpStruct('AVClientStartInConfig', args[0], 256);
        runs.forEach(r => {
          if (RE_UUID.test(r) && r.includes('-')) log('EBO_IDENTITY?', r);
          else if (r.length >= 6) log('EBO_field(account/token?)', r);
        });
      },
    });
    console.log('[*] hooked avClientStartEx');
  }

  if (avIoctl) {
    // avSendIOCtrl(int, unsigned int ioType, const char* data, int len)
    Interceptor.attach(avIoctl, {
      onEnter(args) {
        const ioType = args[1].toInt32() >>> 0;
        if (ioType === 0x9930) {
          const len = args[3].toInt32();
          try {
            const bytes = args[2].readByteArray(len);
            console.log('\n[[EBO]] ioctl9930 payload (' + len + 'B) — save to vendor/ioctl9930.bin:');
            console.log(hexdump(args[2], { length: len, ansi: false }));
          } catch (e) {}
        }
      },
    });
    console.log('[*] hooked avSendIOCtrl (watching for 0x9930)');
  }

  console.log('\n[*] Hooks installed. Now open LIVE VIEW in the app so it ' +
              'connects to the robot. Values will print below, tagged [[EBO]].');
}

function waitForLibs() {
  const present = LIBS.filter(n => Module.findBaseAddress(n) !== null);
  if (present.length) {
    console.log('[*] TUTK libs loaded: ' + present.join(', '));
    attach();
    return;
  }
  // Libs may load lazily; retry until they appear.
  const iv = setInterval(() => {
    if (LIBS.some(n => Module.findBaseAddress(n) !== null)) {
      clearInterval(iv);
      console.log('[*] TUTK libs now loaded.');
      attach();
    }
  }, 500);
}

waitForLibs();
