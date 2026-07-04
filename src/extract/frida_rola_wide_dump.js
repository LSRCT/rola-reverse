'use strict';

/*
 * Wide capture agent for the ROLA Mini phone-free path.
 *
 * Run this when the patched app is logged in. It dumps the app's HTTP/session
 * material plus Agora/RTM control flow. Output contains secrets; tee it only to
 * ignored local artifacts.
 */

const seen = {};
const MAX_VALUE = 60000;

function now() {
  return new Date().toISOString();
}

function s(v) {
  try {
    if (v === null || v === undefined) return '';
    return String(v);
  } catch (e) {
    return '<toString failed: ' + e + '>';
  }
}

function trunc(v) {
  const text = typeof v === 'string' ? v : JSON.stringify(v);
  if (text.length <= MAX_VALUE) return text;
  return text.slice(0, MAX_VALUE) + '...<truncated ' + (text.length - MAX_VALUE) + ' chars>';
}

function emit(tag, value, dedupe) {
  const payload = {
    ts: now(),
    tag,
    value,
  };
  const line = '[[ROLA_WIDE]] ' + tag + ' = ' + trunc(payload);
  if (dedupe) {
    if (seen[line]) return;
    seen[line] = true;
  }
  console.log('\n' + line);
}

function safeCall(fn, fallback) {
  try { return fn(); } catch (e) { return fallback === undefined ? '<err: ' + e + '>' : fallback; }
}

function interesting(text) {
  text = s(text).toLowerCase();
  return text.includes('mini') ||
    text.includes('rola') ||
    text.includes('enabot') ||
    text.includes('ebo.') ||
    text.includes('/api/v1/') ||
    text.includes('rtc') ||
    text.includes('rtm') ||
    text.includes('agora') ||
    text.includes('session') ||
    text.includes('sid') ||
    text.includes('robot') ||
    text.includes('app_rtc') ||
    text.includes('app_rtm') ||
    text.includes('mini_rtc') ||
    text.includes('mini_rtm') ||
    text.includes('token') ||
    text.includes('channel');
}

function mapSummary(map) {
  if (!map) return null;
  return safeCall(() => s(map), '<map unavailable>');
}

function listSummary(list) {
  if (!list) return null;
  return safeCall(() => s(list), '<list unavailable>');
}

function urlOfConnection(conn) {
  return safeCall(() => s(conn.getURL()), '');
}

function requestParamsSummary(params) {
  if (!params) return null;
  return {
    className: safeCall(() => params.getClass().getName(), ''),
    uri_h: safeCall(() => s(params.h()), ''),
    toString: safeCall(() => s(params), ''),
    body_b: safeCall(() => s(params.b.value), ''),
    headers: safeCall(() => mapSummary(params.f.value), ''),
  };
}

function rtcConnectionSummary(conn) {
  if (!conn) return null;
  return {
    channelId: safeCall(() => s(conn.channelId.value), ''),
    localUid: safeCall(() => Number(conn.localUid.value), null),
  };
}

function sessionSummary(obj) {
  if (!obj) return null;
  return {
    sid: safeCall(() => s(obj.getSid()), ''),
    app_rtc_uid: safeCall(() => s(obj.getApp_rtc_uid()), ''),
    app_rtc_token: safeCall(() => s(obj.getApp_rtc_token()), ''),
    app_rtm_uid: safeCall(() => s(obj.getApp_rtm_uid()), ''),
    app_rtm_token: safeCall(() => s(obj.getApp_rtm_token()), ''),
    rtc_channel: safeCall(() => s(obj.getRtc_channel()), ''),
    mini_rtc_uid: safeCall(() => s(obj.getMini_rtc_uid()), ''),
    mini_rtm_uid: safeCall(() => s(obj.getMini_rtm_uid()), ''),
  };
}

function hook(tag, fn) {
  try {
    fn();
    console.log('[wide] hooked ' + tag);
  } catch (e) {
    console.log('[wide] hook failed ' + tag + ': ' + e);
  }
}

function hookAllOverloads(klass, methodName, before) {
  klass[methodName].overloads.forEach(overload => {
    overload.implementation = function () {
      let info = null;
      try {
        const args = [];
        for (let i = 0; i < arguments.length; i++) args.push(arguments[i]);
        info = before.call(this, args, overload);
      } catch (e) {
        emit('HOOK_ERROR.' + methodName, s(e), false);
      }
      const ret = overload.apply(this, arguments);
      if (info && info.after) {
        try { info.after(ret); } catch (e) { emit('HOOK_AFTER_ERROR.' + methodName, s(e), false); }
      }
      return ret;
    };
  });
}

function install() {
  Java.perform(() => {
    emit('START', 'wide dump hooks installing', true);

    hook('loaded class inventory', () => {
      const classes = Java.enumerateLoadedClassesSync()
        .filter(name =>
          name.indexOf('com.ebo.ebocode.deviceRolaMini') === 0 ||
          name.indexOf('com.enabot.lib_device.agora') === 0 ||
          name.indexOf('org.xutils') === 0 ||
          name.indexOf('okhttp3') === 0 ||
          name.indexOf('retrofit2') === 0 ||
          name.indexOf('anet.channel') === 0 ||
          name.indexOf('anetwork.channel') === 0)
        .sort();
      emit('LOADED_CLASSES_INTERESTING', classes, true);
    });

    hook('Mini getRolaMiniSession callback', () => {
      const Cb = Java.use('com.ebo.ebocode.deviceRolaMini.RolaMiniConnectModel$getRolaMiniSession$1');
      Cb.onSuccess.overload('java.lang.Object').implementation = function (result) {
        emit('MINI_SESSION_HTTP_SUCCESS_RAW', s(result), false);
        return this.onSuccess(result);
      };
      Cb.a.overload('java.lang.Throwable', 'boolean').implementation = function (err, flag) {
        emit('MINI_SESSION_HTTP_ERROR', { error: s(err), flag: !!flag }, false);
        return this.a(err, flag);
      };
    });

    hook('Mini session object constructor/getters', () => {
      const Session = Java.use('j4.g');
      Session.$init.overload(
        'java.lang.String',
        'java.lang.String',
        'java.lang.String',
        'java.lang.String',
        'java.lang.String',
        'java.lang.String',
        'java.lang.String',
        'java.lang.String',
        'boolean'
      ).implementation = function (sid, appRtcUid, appRtcToken, appRtmUid, appRtmToken, rtcChannel, miniRtcUid, miniRtmUid, isOnline) {
        emit('MINI_SESSION_CONSTRUCTOR', {
          sid: s(sid),
          app_rtc_uid: s(appRtcUid),
          app_rtc_token: s(appRtcToken),
          app_rtm_uid: s(appRtmUid),
          app_rtm_token: s(appRtmToken),
          rtc_channel: s(rtcChannel),
          mini_rtc_uid: s(miniRtcUid),
          mini_rtm_uid: s(miniRtmUid),
          is_online: !!isOnline,
        }, false);
        return this.$init(sid, appRtcUid, appRtcToken, appRtmUid, appRtmToken, rtcChannel, miniRtcUid, miniRtmUid, isOnline);
      };
      ['getSid', 'getApp_rtc_uid', 'getApp_rtc_token', 'getApp_rtm_uid', 'getApp_rtm_token', 'getRtc_channel', 'getMini_rtc_uid', 'getMini_rtm_uid'].forEach(methodName => {
        const m = Session[methodName].overload();
        m.implementation = function () {
          const ret = m.call(this);
          emit('MINI_SESSION_GETTER.' + methodName, s(ret), true);
          return ret;
        };
      });
    });

    hook('xutils MyHttpManager', () => {
      const M = Java.use('com.ebo.ebocode.base.MyHttpManager');
      M.b.overload('org.xutils.http.HttpMethod', 'org.xutils.http.RequestParams', 'org.xutils.common.Callback$CommonCallback')
        .implementation = function (method, params, callback) {
          emit('XUTILS_REQUEST', {
            method: s(method),
            params: requestParamsSummary(params),
            callback: safeCall(() => callback.getClass().getName(), ''),
          }, false);
          return this.b(method, params, callback);
        };
    });

    hook('xutils RequestParams mutators', () => {
      const RP = Java.use('org.xutils.http.RequestParams');
      hookAllOverloads(RP, '$init', function (args) {
        emit('XUTILS_REQUESTPARAMS_INIT', args.map(s), false);
      });
    });

    hook('xutils BaseParams header setter', () => {
      const BP = Java.use('org.xutils.http.BaseParams');
      const setHeader = BP.f.overload('java.lang.String', 'java.lang.String');
      setHeader.implementation = function (name, value) {
        if (interesting(name) || interesting(value)) {
          emit('XUTILS_BASEPARAMS_HEADER_SET', { name: s(name), value: s(value) }, false);
        }
        return setHeader.call(this, name, value);
      };
    });

    hook('xutils final HttpRequest', () => {
      const HR = Java.use('org.xutils.http.request.HttpRequest');
      const M = HR.M.overload();
      M.implementation = function () {
        const beforeUrl = safeCall(() => s(this.F()), '');
        if (interesting(beforeUrl)) {
          emit('XUTILS_HTTPREQUEST_M_ENTER', {
            url: beforeUrl,
            params: requestParamsSummary(this.c.value),
          }, false);
        }
        try {
          const ret = M.call(this);
          const conn = safeCall(() => this.k.value, null);
          const finalUrl = conn ? urlOfConnection(conn) : beforeUrl;
          if (interesting(finalUrl) || interesting(beforeUrl)) {
            emit('XUTILS_HTTPREQUEST_M_EXIT', {
              url: finalUrl,
              method: safeCall(() => s(conn.getRequestMethod()), ''),
              responseCode: safeCall(() => Number(this.l.value), null),
              requestProperties: safeCall(() => mapSummary(conn.getRequestProperties()), '<request properties unavailable after connect>'),
              responseHeaders: safeCall(() => mapSummary(conn.getHeaderFields()), ''),
            }, false);
          }
          return ret;
        } catch (e) {
          const conn = safeCall(() => this.k.value, null);
          emit('XUTILS_HTTPREQUEST_M_THROW', {
            url: conn ? urlOfConnection(conn) : beforeUrl,
            error: s(e),
            responseCode: safeCall(() => Number(this.l.value), null),
            responseHeaders: safeCall(() => conn ? mapSummary(conn.getHeaderFields()) : '', ''),
          }, false);
          throw e;
        }
      };
    });

    hook('java CookieManager', () => {
      const CM = Java.use('java.net.CookieManager');
      const get = CM.get.overload('java.net.URI', 'java.util.Map');
      get.implementation = function (uri, headers) {
        const ret = get.call(this, uri, headers);
        const uriText = s(uri);
        if (interesting(uriText) || interesting(ret)) {
          emit('COOKIE_MANAGER_GET', {
            uri: uriText,
            requestHeaders: mapSummary(headers),
            result: mapSummary(ret),
          }, false);
        }
        return ret;
      };
      const put = CM.put.overload('java.net.URI', 'java.util.Map');
      put.implementation = function (uri, headers) {
        const uriText = s(uri);
        if (interesting(uriText) || interesting(headers)) {
          emit('COOKIE_MANAGER_PUT', {
            uri: uriText,
            responseHeaders: mapSummary(headers),
          }, false);
        }
        return put.call(this, uri, headers);
      };
    });

    hook('java URLConnection request properties', () => {
      const UC = Java.use('java.net.URLConnection');
      const set = UC.setRequestProperty.overload('java.lang.String', 'java.lang.String');
      set.implementation = function (name, value) {
        const url = urlOfConnection(this);
        if (interesting(url) || interesting(name) || interesting(value)) {
          emit('URLCONN_SET_REQUEST_PROPERTY', { url, name: s(name), value: s(value) }, false);
        }
        return set.call(this, name, value);
      };
      const add = UC.addRequestProperty.overload('java.lang.String', 'java.lang.String');
      add.implementation = function (name, value) {
        const url = urlOfConnection(this);
        if (interesting(url) || interesting(name) || interesting(value)) {
          emit('URLCONN_ADD_REQUEST_PROPERTY', { url, name: s(name), value: s(value) }, false);
        }
        return add.call(this, name, value);
      };
    });

    hook('java HttpURLConnection request method', () => {
      const HUC = Java.use('java.net.HttpURLConnection');
      const setMethod = HUC.setRequestMethod.overload('java.lang.String');
      setMethod.implementation = function (method) {
        const url = urlOfConnection(this);
        if (interesting(url)) emit('URLCONN_SET_REQUEST_METHOD', { url, method: s(method) }, false);
        return setMethod.call(this, method);
      };
    });

    hook('xutils StringBody write', () => {
      const JString = Java.use('java.lang.String');
      const Body = Java.use('org.xutils.http.body.StringBody');
      const writeTo = Body.writeTo.overload('java.io.OutputStream');
      writeTo.implementation = function (out) {
        emit('XUTILS_STRING_BODY_WRITE', {
          contentType: safeCall(() => s(this.getContentType()), ''),
          length: safeCall(() => this.a.value.length, 0),
          body: safeCall(() => s(JString.$new(this.a.value, 'UTF-8')), '<body unavailable>'),
        }, false);
        return writeTo.call(this, out);
      };
    });

    hook('anet request submit', () => {
      const Session = Java.use('anet.channel.Session');
      Session.request.overload('anet.channel.request.Request', 'anet.channel.RequestCb').implementation = function (req, cb) {
        emit('ANET_REQUEST', {
          url: safeCall(() => s(req.getUrlString()), ''),
          headers: safeCall(() => mapSummary(req.getHeaders()), ''),
          method: safeCall(() => s(req.getMethod()), ''),
          bizId: safeCall(() => s(req.getBizId()), ''),
          callback: safeCall(() => cb.getClass().getName(), ''),
        }, false);
        return this.request(req, cb);
      };
    });

    hook('java URL openConnection', () => {
      const URL = Java.use('java.net.URL');
      URL.openConnection.overload().implementation = function () {
        const url = s(this.toString());
        if (interesting(url)) emit('URL_OPEN_CONNECTION', url, false);
        return this.openConnection();
      };
      URL.openConnection.overload('java.net.Proxy').implementation = function (proxy) {
        const url = s(this.toString());
        if (interesting(url)) emit('URL_OPEN_CONNECTION_PROXY', { url, proxy: s(proxy) }, false);
        return this.openConnection(proxy);
      };
    });

    hook('Gson parse helpers', () => {
      const H = Java.use('w4.h');
      H.c.overload('java.lang.Class', 'java.lang.String').implementation = function (klass, json) {
        const klassName = safeCall(() => klass.getName(), '');
        if (interesting(klassName) || interesting(json)) {
          emit('JSON_HELPER_FROM_STRING', { className: klassName, json: s(json) }, false);
        }
        return this.c(klass, json);
      };
      H.b.overload('com.google.gson.g', 'java.lang.reflect.Type').implementation = function (jsonElement, type) {
        const typeName = s(type);
        const json = s(jsonElement);
        if (interesting(typeName) || interesting(json)) {
          emit('JSON_HELPER_FROM_ELEMENT', { type: typeName, json }, false);
        }
        return this.b(jsonElement, type);
      };
      H.e.overload('java.lang.Object').implementation = function (obj) {
        const ret = this.e(obj);
        if (interesting(ret) || safeCall(() => obj.getClass().getName(), '').indexOf('RolaMini') >= 0) {
          emit('JSON_HELPER_TO_STRING', {
            objectClass: safeCall(() => obj.getClass().getName(), ''),
            json: s(ret),
          }, false);
        }
        return ret;
      };
    });

    hook('Agora RTC join and RDT', () => {
      const RtcEngineEx = Java.use('io.agora.rtc2.RtcEngineEx');
      RtcEngineEx.joinChannelEx.overload(
        'java.lang.String',
        'io.agora.rtc2.RtcConnection',
        'io.agora.rtc2.ChannelMediaOptions',
        'io.agora.rtc2.IRtcEngineEventHandler'
      ).implementation = function (token, conn, opts, handler) {
        emit('AGORA_RTC_JOIN', { token: s(token), connection: rtcConnectionSummary(conn), options: s(opts) }, false);
        return this.joinChannelEx(token, conn, opts, handler);
      };
      RtcEngineEx.sendRdtMessageEx.overload('int', 'int', '[B', 'io.agora.rtc2.RtcConnection')
        .implementation = function (uid, type, bytes, conn) {
          emit('AGORA_RDT_SEND', {
            uid,
            type,
            len: safeCall(() => bytes.length, 0),
            ascii: safeCall(() => Java.use('java.lang.String').$new(bytes), ''),
            connection: rtcConnectionSummary(conn),
          }, false);
          return this.sendRdtMessageEx(uid, type, bytes, conn);
        };
    });

    hook('Agora RTM login/publish', () => {
      const RtmClient = Java.use('io.agora.rtm.RtmClient');
      RtmClient.login.overload('java.lang.String', 'io.agora.rtm.ResultCallback').implementation = function (token, cb) {
        emit('AGORA_RTM_LOGIN', { token: s(token), callback: safeCall(() => cb.getClass().getName(), '') }, false);
        return this.login(token, cb);
      };
      RtmClient.publish.overload('java.lang.String', 'java.lang.String', 'io.agora.rtm.PublishOptions', 'io.agora.rtm.ResultCallback')
        .implementation = function (peerId, message, opts, cb) {
          emit('AGORA_RTM_PUBLISH', { peerId: s(peerId), message: s(message), options: s(opts) }, false);
          return this.publish(peerId, message, opts, cb);
        };
    });

    hook('Mini RTM wrapper send path', () => {
      const Wrap = Java.use('com.ebo.ebocode.deviceRolaMini.q');
      Wrap.x.overload('androidx.collection.ArrayMap', 'com.enabot.lib_device.agora.k').implementation = function (message, cb) {
        emit('MINI_RTM_MAP_SEND', s(message), false);
        return this.x(message, cb);
      };
    });

    hook('heap snapshot helper', () => {
      setTimeout(() => {
        Java.perform(() => {
          Java.choose('j4.g', {
            onMatch(obj) {
              emit('HEAP_MINI_SESSION_OBJECT', sessionSummary(obj), false);
            },
            onComplete() {
              emit('HEAP_MINI_SESSION_SCAN_COMPLETE', 'done', true);
            },
          });
        });
      }, 2500);
    });

    emit('READY', 'wide dump hooks installed; trigger live view/session refresh now', true);
  });
}

if (typeof Java === 'undefined' || !Java.available) {
  console.log('[wide] Java is not available in this Frida realm');
} else {
  install();
}
