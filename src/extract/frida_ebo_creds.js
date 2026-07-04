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

function moduleByName(name) {
  try { return Process.getModuleByName(name); }
  catch (e) { return null; }
}

function moduleLoaded(name) {
  return moduleByName(name) !== null;
}

function exportByName(name) {
  try { return Module.getGlobalExportByName(name); }
  catch (e) {}
  for (const lib of LIBS) {
    const m = moduleByName(lib);
    if (!m) continue;
    try { return m.getExportByName(name); }
    catch (e) {}
  }
  return null;
}

function log(tag, val) {
  const key = tag + '=' + val;
  if (found[key]) return;
  found[key] = true;
  console.log('\n[[EBO]] ' + tag + ' = ' + JSON.stringify(val));
}

function trace(tag, val) {
  console.log('\n[[EBO]] ' + tag + ' = ' + JSON.stringify(val));
}

function safeString(v) {
  try {
    if (v === null || v === undefined) return '';
    return String(v);
  } catch (e) {
    return '<string failed: ' + e + '>';
  }
}

function safeField(obj, name) {
  try {
    if (!obj) return '';
    const f = obj[name];
    if (!f) return '';
    return safeString(f.value);
  } catch (e) {
    return '';
  }
}

function rtcConnectionSummary(conn) {
  return {
    channelId: safeField(conn, 'channelId'),
    localUid: safeField(conn, 'localUid'),
  };
}

function javaBytesToArray(arr) {
  const out = [];
  try {
    if (!arr) return out;
    for (let i = 0; i < arr.length; i++) out.push(arr[i] & 0xff);
  } catch (e) {}
  return out;
}

function bytesSummary(arr) {
  const bytes = javaBytesToArray(arr);
  let hex = '';
  let ascii = '';
  for (const b of bytes) {
    hex += (b < 16 ? '0' : '') + b.toString(16);
    ascii += (b >= 0x20 && b < 0x7f) ? String.fromCharCode(b) : '.';
  }
  return { len: bytes.length, hex, ascii };
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
  const iotc = exportByName('IOTC_Connect_ByUIDEx');
  const avStart = exportByName('avClientStartEx');
  const setLic = exportByName('TUTK_SDK_Set_License_Key') ||
                 exportByName('TUTKGlobal_SDK_Set_License_Key');
  const avIoctl = exportByName('avSendIOCtrl');

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

function installJavaHooks() {
  if (typeof Java === 'undefined' || !Java.available) return;
  Java.perform(() => {
    try {
      const T = Java.use('com.tutk.IOTC.TUTKGlobalAPIs');
      T.TUTK_SDK_Set_License_Key.implementation = function (lic) {
        log('EBO_LICENSE(Java)', String(lic));
        return this.TUTK_SDK_Set_License_Key(lic);
      };
      console.log('[*] hooked Java TUTK_SDK_Set_License_Key');
    } catch (e) {
      console.log('[!] Java TUTKGlobal hook failed: ' + e);
    }

    try {
      const I = Java.use('com.tutk.IOTC.IOTCAPIs');
      I.IOTC_Connect_ByUIDEx.implementation = function (uid, sid, input) {
        log('EBO_UID(Java)', String(uid));
        try { log('EBO_AUTHKEY(Java)', String(input.authKey.value)); } catch (e) {}
        try {
          log('IOTC_INPUT(Java)', JSON.stringify({
            sid,
            authenticationType: input.authenticationType.value,
            deviceRegion: String(input.deviceRegion.value),
            timeout: input.timeout.value,
          }));
        } catch (e) {}
        return this.IOTC_Connect_ByUIDEx(uid, sid, input);
      };
      console.log('[*] hooked Java IOTC_Connect_ByUIDEx');
    } catch (e) {
      console.log('[!] Java IOTC hook failed: ' + e);
    }

    try {
      const A = Java.use('com.tutk.IOTC.AVAPIs');
      A.avClientStartEx.implementation = function (cfg, out) {
        try { log('EBO_IDENTITY(Java)', String(cfg.account_or_identity.value)); } catch (e) {}
        try { log('EBO_TOKEN(Java)', String(cfg.password_or_token.value)); } catch (e) {}
        try {
          log('AV_CONFIG(Java)', JSON.stringify({
            session: cfg.iotc_session_id.value,
            channel: cfg.iotc_channel_id.value,
            auth_type: cfg.auth_type.value,
            security_mode: cfg.security_mode.value,
            timeout_sec: cfg.timeout_sec.value,
            resend: cfg.resend.value,
            sync_recv_data: cfg.sync_recv_data.value,
            cipher: String(cfg.dtls_cipher_suites.value),
          }));
        } catch (e) {}
        return this.avClientStartEx(cfg, out);
      };
      console.log('[*] hooked Java avClientStartEx');
    } catch (e) {
      console.log('[!] Java AV hook failed: ' + e);
    }

    installRolaMiniAgoraHooks();
  });
}

function installRolaMiniAgoraHooks() {
  const hookGetter = (klass, methodName, tag) => {
    try {
      const m = klass[methodName].overload();
      m.implementation = function () {
        const v = m.call(this);
        log(tag, safeString(v));
        return v;
      };
    } catch (e) {
      console.log('[!] hook ' + methodName + ' failed: ' + e);
    }
  };

  try {
    const Session = Java.use('j4.g');
    hookGetter(Session, 'getSid', 'ROLA_MINI_SESSION.sid');
    hookGetter(Session, 'getApp_rtc_uid', 'ROLA_MINI_SESSION.app_rtc_uid');
    hookGetter(Session, 'getApp_rtc_token', 'ROLA_MINI_SESSION.app_rtc_token');
    hookGetter(Session, 'getApp_rtm_uid', 'ROLA_MINI_SESSION.app_rtm_uid');
    hookGetter(Session, 'getApp_rtm_token', 'ROLA_MINI_SESSION.app_rtm_token');
    hookGetter(Session, 'getRtc_channel', 'ROLA_MINI_SESSION.rtc_channel');
    hookGetter(Session, 'getMini_rtc_uid', 'ROLA_MINI_SESSION.mini_rtc_uid');
    hookGetter(Session, 'getMini_rtm_uid', 'ROLA_MINI_SESSION.mini_rtm_uid');
    console.log('[*] hooked ROLA Mini session getters');
  } catch (e) {
    console.log('[!] ROLA Mini session hook failed: ' + e);
  }

  try {
    const B0 = Java.use('com.enabot.lib_device.agora.camera.b0');
    const d = B0.d.overload(
      'com.enabot.lib_device.agora.camera.b0',
      'java.lang.String',
      'com.ebo.ebocode.deviceRolaMini.m',
      'int'
    );
    d.implementation = function (engine, token, callback, flags) {
      log('ROLA_MINI_RTC_TOKEN_ARG', safeString(token));
      try { log('ROLA_MINI_RTC_CONNECTION_ARG', rtcConnectionSummary(engine.a.value)); } catch (e) {}
      return d.call(this, engine, token, callback, flags);
    };
    console.log('[*] hooked Agora camera join wrapper');
  } catch (e) {
    console.log('[!] Agora camera join wrapper hook failed: ' + e);
  }

  try {
    const JoinTask = Java.use('com.enabot.lib_device.agora.camera.l');
    const init = JoinTask.$init.overload(
      'java.lang.String',
      'io.agora.rtc2.RtcConnection',
      'io.agora.rtc2.IRtcEngineEventHandler',
      'kotlin.jvm.functions.Function0',
      'kotlin.jvm.functions.Function0'
    );
    init.implementation = function (token, conn, handler, before, after) {
      log('ROLA_MINI_RTC_TOKEN_TASK', safeString(token));
      log('ROLA_MINI_RTC_CONNECTION_TASK', rtcConnectionSummary(conn));
      return init.call(this, token, conn, handler, before, after);
    };
    console.log('[*] hooked Agora join task constructor');
  } catch (e) {
    console.log('[!] Agora join task constructor hook failed: ' + e);
  }

  try {
    const RtcEngineEx = Java.use('io.agora.rtc2.RtcEngineEx');
    const join = RtcEngineEx.joinChannelEx.overload(
      'java.lang.String',
      'io.agora.rtc2.RtcConnection',
      'io.agora.rtc2.ChannelMediaOptions',
      'io.agora.rtc2.IRtcEngineEventHandler'
    );
    join.implementation = function (token, conn, opts, handler) {
      log('AGORA_RTC_JOIN_TOKEN', safeString(token));
      log('AGORA_RTC_JOIN_CONNECTION', rtcConnectionSummary(conn));
      return join.call(this, token, conn, opts, handler);
    };

    const sendRdt = RtcEngineEx.sendRdtMessageEx.overload(
      'int',
      'int',
      '[B',
      'io.agora.rtc2.RtcConnection'
    );
    sendRdt.implementation = function (uid, type, message, conn) {
      trace('AGORA_RDT_SEND', {
        uid,
        type,
        message: bytesSummary(message),
        connection: rtcConnectionSummary(conn),
      });
      return sendRdt.call(this, uid, type, message, conn);
    };
    console.log('[*] hooked Agora RTC join/sendRdt');
  } catch (e) {
    console.log('[!] Agora RTC hook failed: ' + e);
  }

  try {
    const RtmLoginTask = Java.use('com.enabot.lib_device.agora.camera.w');
    const init = RtmLoginTask.$init.overload(
      'java.lang.String',
      'java.lang.String',
      'com.enabot.lib_device.agora.j'
    );
    init.implementation = function (rtmToken, uid, callback) {
      log('ROLA_MINI_RTM_TOKEN_TASK', safeString(rtmToken));
      log('ROLA_MINI_RTM_UID_TASK', safeString(uid));
      return init.call(this, rtmToken, uid, callback);
    };
    console.log('[*] hooked Agora RTM login task constructor');
  } catch (e) {
    console.log('[!] Agora RTM login task hook failed: ' + e);
  }

  try {
    const RtmClient = Java.use('io.agora.rtm.RtmClient');
    const login = RtmClient.login.overload('java.lang.String', 'io.agora.rtm.ResultCallback');
    login.implementation = function (token, callback) {
      log('AGORA_RTM_LOGIN_TOKEN', safeString(token));
      return login.call(this, token, callback);
    };

    const publish = RtmClient.publish.overload(
      'java.lang.String',
      'java.lang.String',
      'io.agora.rtm.PublishOptions',
      'io.agora.rtm.ResultCallback'
    );
    publish.implementation = function (peerId, message, options, callback) {
      trace('AGORA_RTM_PUBLISH', {
        peerId: safeString(peerId),
        message: safeString(message),
      });
      return publish.call(this, peerId, message, options, callback);
    };
    console.log('[*] hooked Agora RTM login/publish');
  } catch (e) {
    console.log('[!] Agora RTM hook failed: ' + e);
  }

  try {
    const PublishTask = Java.use('com.enabot.lib_device.agora.camera.x');
    const init = PublishTask.$init.overload(
      'java.lang.String',
      'java.lang.String',
      'com.enabot.lib_device.agora.k'
    );
    init.implementation = function (message, peerId, callback) {
      trace('ROLA_MINI_RTM_PUBLISH_TASK', {
        peerId: safeString(peerId),
        message: safeString(message),
      });
      return init.call(this, message, peerId, callback);
    };
    console.log('[*] hooked app RTM publish task constructor');
  } catch (e) {
    console.log('[!] App RTM publish task hook failed: ' + e);
  }

  try {
    const RdtTask = Java.use('com.enabot.lib_device.agora.camera.y');
    const init = RdtTask.$init.overload('int', 'int', '[B', 'io.agora.rtc2.RtcConnection');
    init.implementation = function (uid, type, message, conn) {
      trace('ROLA_MINI_RDT_TASK', {
        uid,
        type,
        message: bytesSummary(message),
        connection: rtcConnectionSummary(conn),
      });
      return init.call(this, uid, type, message, conn);
    };
    console.log('[*] hooked app RDT task constructor');
  } catch (e) {
    console.log('[!] App RDT task hook failed: ' + e);
  }

  try {
    const Wrap = Java.use('com.ebo.ebocode.deviceRolaMini.q');
    const x = Wrap.x.overload('androidx.collection.ArrayMap', 'com.enabot.lib_device.agora.k');
    x.implementation = function (message, callback) {
      trace('ROLA_MINI_Q_X_RTM_MAP', safeString(message));
      return x.call(this, message, callback);
    };

    const y = Wrap.y.overload('com.ebo.ebocode.deviceRolaMini.q', '[B');
    y.implementation = function (wrap, message) {
      trace('ROLA_MINI_Q_Y_RDT_BYTES', bytesSummary(message));
      return y.call(this, wrap, message);
    };
    console.log('[*] hooked ROLA Mini wrapper send paths');
  } catch (e) {
    console.log('[!] ROLA Mini wrapper send hook failed: ' + e);
  }

  try {
    const LiveModel = Java.use('com.ebo.ebocode.deviceRolaMini.live.RolaMiniLiveModel');
    const move = LiveModel.l.overload('int', 'int', 'int');
    move.implementation = function (ly, rx, buttons) {
      trace('ROLA_MINI_MOVE', { lx: 0, ly, rx, ry: 0, buttons, id: 101007 });
      return move.call(this, ly, rx, buttons);
    };
    console.log('[*] hooked ROLA Mini movement builder');
  } catch (e) {
    console.log('[!] ROLA Mini movement hook failed: ' + e);
  }

  console.log('[*] ROLA Mini Agora/RTM hooks installed');
}

function waitForLibs() {
  installJavaHooks();
  const present = LIBS.filter(n => moduleLoaded(n));
  if (present.length) {
    console.log('[*] TUTK libs loaded: ' + present.join(', '));
    attach();
    return;
  }
  // Libs may load lazily; retry until they appear.
  const iv = setInterval(() => {
    if (LIBS.some(n => moduleLoaded(n))) {
      clearInterval(iv);
      console.log('[*] TUTK libs now loaded.');
      attach();
    }
  }, 500);
}

waitForLibs();
