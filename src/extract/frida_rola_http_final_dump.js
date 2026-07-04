'use strict';

/*
 * Focused HTTP-final capture for the ROLA Mini session endpoint.
 *
 * This intentionally avoids the broad Agora/JSON/class-inventory hooks from
 * frida_rola_wide_dump.js. It only captures xutils final request material:
 * cookies, URLConnection request properties, JSON body, response code/headers,
 * and the Mini session callback response.
 */

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

function emit(tag, value) {
  console.log('\n[[ROLA_HTTP_FINAL]] ' + tag + ' = ' + trunc({
    ts: now(),
    tag,
    value,
  }));
}

function safeCall(fn, fallback) {
  try { return fn(); } catch (e) { return fallback === undefined ? '<err: ' + e + '>' : fallback; }
}

function interesting(text) {
  text = s(text).toLowerCase();
  return text.includes('enabot') ||
    text.includes('ebo.') ||
    text.includes('/api/v1/') ||
    text.includes('mini') ||
    text.includes('rola') ||
    text.includes('session') ||
    text.includes('robot') ||
    text.includes('sid') ||
    text.includes('token') ||
    text.includes('app_rtc') ||
    text.includes('app_rtm') ||
    text.includes('mini_rtc') ||
    text.includes('mini_rtm');
}

function mapSummary(map) {
  if (!map) return null;
  return safeCall(() => s(map), '<map unavailable>');
}

function listSummary(list) {
  if (!list) return null;
  return safeCall(() => {
    const size = Number(list.size());
    const items = [];
    const limit = Math.min(size, 25);
    for (let i = 0; i < limit; i += 1) {
      const item = list.get(i);
      items.push(objectSummary(item));
    }
    return {
      className: safeCall(() => list.getClass().getName(), ''),
      size,
      truncated: size > limit,
      items,
      toString: s(list),
    };
  }, '<list unavailable>');
}

function objectSummary(obj) {
  if (!obj) return null;
  return {
    className: safeCall(() => obj.getClass().getName(), ''),
    toString: safeCall(() => obj.toString(), s(obj)),
    cookieAccessors: cookieAccessors(obj),
    fields: reflectedFields(obj, 40),
  };
}

function cookieAccessors(obj) {
  const out = {};
  [
    'name',
    'value',
    'domain',
    'path',
    'expiresAt',
    'secure',
    'httpOnly',
    'hostOnly',
    'persistent',
  ].forEach((name) => {
    const value = safeCall(() => obj[name](), undefined);
    if (value !== undefined) out[name] = s(value);
  });
  return out;
}

function reflectedFields(obj, maxFields) {
  return safeCall(() => {
    const cls = obj.getClass();
    const fields = cls.getDeclaredFields();
    const out = [];
    const limit = Math.min(fields.length, maxFields);
    for (let i = 0; i < limit; i += 1) {
      const field = fields[i];
      out.push({
        name: safeCall(() => s(field.getName()), ''),
        type: safeCall(() => s(field.getType().getName()), ''),
        value: safeCall(() => {
          field.setAccessible(true);
          return s(field.get(obj));
        }, '<unavailable>'),
      });
    }
    if (fields.length > limit) {
      out.push({ truncated: fields.length - limit });
    }
    return out;
  }, '<fields unavailable>');
}

function urlOfConnection(conn) {
  return safeCall(() => s(conn.getURL()), '');
}

function requestSummary(req) {
  if (!req) return null;
  return {
    toString: safeCall(() => s(req), ''),
    url: safeCall(() => s(req.a.value), ''),
    method: safeCall(() => s(req.b.value), ''),
    headers: safeCall(() => s(req.c.value), ''),
    body: safeCall(() => s(req.d.value), ''),
    tags: safeCall(() => s(req.e.value), ''),
  };
}

function requestParamsSummary(params) {
  if (!params) return null;
  return {
    className: safeCall(() => params.getClass().getName(), ''),
    uri: safeCall(() => s(params.h()), ''),
    toString: safeCall(() => s(params), ''),
    method: safeCall(() => s(params.a.value), ''),
    rawBody: safeCall(() => s(params.b.value), ''),
    contentType: safeCall(() => s(params.c.value), ''),
    headers: safeCall(() => mapSummary(params.f.value), ''),
    queryItems: safeCall(() => mapSummary(params.g.value), ''),
    bodyItems: safeCall(() => mapSummary(params.h.value), ''),
    cookieEnabled: safeCall(() => !!params.p.value, null),
  };
}

function hook(tag, fn) {
  try {
    fn();
    console.log('[http-final] hooked ' + tag);
  } catch (e) {
    console.log('[http-final] hook failed ' + tag + ': ' + e);
  }
}

function triggerMiniSession() {
  let attempt = 0;
  function tryOnce() {
    attempt += 1;
    Java.perform(() => {
      let found = false;
      Java.choose('com.ebo.ebocode.deviceRolaMini.w', {
        onMatch(obj) {
          found = true;
          emit('AUTO_TRIGGER_MINI_SESSION', {
            attempt,
            action: 'RolaMiniConnectModel.a(true)',
          });
          obj.a(true);
          return 'stop';
        },
        onComplete() {
          emit('AUTO_TRIGGER_SCAN_COMPLETE', { attempt, found });
          if (!found && attempt < 8) setTimeout(tryOnce, 1500);
        },
      });
    });
  }
  setTimeout(tryOnce, 2500);
}

function install() {
  Java.perform(() => {
    emit('START', 'focused HTTP final hooks installing');

    hook('Mini session callback', () => {
      const Cb = Java.use('com.ebo.ebocode.deviceRolaMini.RolaMiniConnectModel$getRolaMiniSession$1');
      Cb.onSuccess.overload('java.lang.Object').implementation = function (result) {
        emit('MINI_SESSION_HTTP_SUCCESS_RAW', s(result));
        return this.onSuccess(result);
      };
      Cb.a.overload('java.lang.Throwable', 'boolean').implementation = function (err, flag) {
        emit('MINI_SESSION_HTTP_ERROR', { error: s(err), flag: !!flag });
        return this.a(err, flag);
      };
    });

    hook('xutils MyHttpManager', () => {
      const Mgr = Java.use('com.ebo.ebocode.base.MyHttpManager');
      const post = Mgr.b.overload('org.xutils.http.HttpMethod', 'org.xutils.http.RequestParams', 'org.xutils.common.Callback$CommonCallback');
      post.implementation = function (method, params, callback) {
        const summary = requestParamsSummary(params);
        if (interesting(summary.uri) || interesting(summary.toString)) {
          emit('XUTILS_MANAGER_REQUEST', {
            method: s(method),
            params: summary,
            callback: safeCall(() => callback.getClass().getName(), ''),
          });
        }
        return post.call(this, method, params, callback);
      };
    });

    hook('app OkHttp request builder', () => {
      const Ok = Java.use('com.ebo.ebocode.okhttp.j');
      const buildJson = Ok.f.overload('okhttp3.z', 'java.lang.String', 'int', 'java.util.HashMap', 'java.lang.Object');
      buildJson.implementation = function (url, body, methodId, headers, tagObj) {
        if (interesting(url) || interesting(body) || interesting(headers)) {
          emit('OKHTTP_BUILD_JSON_ENTER', {
            url: s(url),
            body: s(body),
            methodId,
            headers: mapSummary(headers),
            tag: s(tagObj),
          });
        }
        const req = buildJson.call(this, url, body, methodId, headers, tagObj);
        if (interesting(req)) {
          emit('OKHTTP_BUILD_JSON_EXIT', requestSummary(req));
        }
        return req;
      };

      const async = Ok.j.overload('okhttp3.l0', 'com.google.android.gms.internal.mlkit_vision_barcode.qb');
      async.implementation = function (req, cb) {
        if (interesting(req)) {
          emit('OKHTTP_ASYNC_SUBMIT', {
            request: requestSummary(req),
            callback: safeCall(() => cb.getClass().getName(), ''),
          });
        }
        return async.call(this, req, cb);
      };
    });

    hook('app OkHttp cookie jar', () => {
      const Jar = Java.use('com.google.android.gms.internal.mlkit_common.z');
      const load = Jar.b.overload('okhttp3.z');
      load.implementation = function (url) {
        const ret = load.call(this, url);
        if (interesting(url) || interesting(ret)) {
          emit('OKHTTP_COOKIE_LOAD', {
            mode: safeCall(() => Number(this.b.value), null),
            url: s(url),
            cookies: listSummary(ret),
          });
        }
        return ret;
      };
      const save = Jar.c.overload('okhttp3.z', 'java.util.List');
      save.implementation = function (url, cookies) {
        if (interesting(url) || interesting(cookies)) {
          emit('OKHTTP_COOKIE_SAVE', {
            mode: safeCall(() => Number(this.b.value), null),
            url: s(url),
            cookies: listSummary(cookies),
          });
        }
        return save.call(this, url, cookies);
      };
    });

    hook('xutils final HttpRequest.M', () => {
      const HR = Java.use('org.xutils.http.request.HttpRequest');
      const M = HR.M.overload();
      M.implementation = function () {
        const beforeUrl = safeCall(() => s(this.F()), '');
        const params = safeCall(() => requestParamsSummary(this.c.value), null);
        const logThis = interesting(beforeUrl) || interesting(params);
        if (logThis) emit('HTTPREQUEST_ENTER', { url: beforeUrl, params });
        try {
          const ret = M.call(this);
          const conn = safeCall(() => this.k.value, null);
          const finalUrl = conn ? urlOfConnection(conn) : beforeUrl;
          if (logThis || interesting(finalUrl)) {
            emit('HTTPREQUEST_EXIT', {
              url: finalUrl,
              method: safeCall(() => s(conn.getRequestMethod()), ''),
              responseCode: safeCall(() => Number(this.l.value), null),
              requestProperties: safeCall(() => mapSummary(conn.getRequestProperties()), '<request properties unavailable after connect>'),
              responseHeaders: safeCall(() => mapSummary(conn.getHeaderFields()), ''),
            });
          }
          return ret;
        } catch (e) {
          const conn = safeCall(() => this.k.value, null);
          emit('HTTPREQUEST_THROW', {
            url: conn ? urlOfConnection(conn) : beforeUrl,
            error: s(e),
            responseCode: safeCall(() => Number(this.l.value), null),
            responseHeaders: safeCall(() => conn ? mapSummary(conn.getHeaderFields()) : '', ''),
          });
          throw e;
        }
      };
    });

    hook('CookieManager', () => {
      const CM = Java.use('java.net.CookieManager');
      const get = CM.get.overload('java.net.URI', 'java.util.Map');
      get.implementation = function (uri, headers) {
        const ret = get.call(this, uri, headers);
        const uriText = s(uri);
        if (interesting(uriText) || interesting(ret)) {
          emit('COOKIE_GET', {
            uri: uriText,
            requestHeaders: mapSummary(headers),
            result: mapSummary(ret),
          });
        }
        return ret;
      };
      const put = CM.put.overload('java.net.URI', 'java.util.Map');
      put.implementation = function (uri, headers) {
        const uriText = s(uri);
        if (interesting(uriText) || interesting(headers)) {
          emit('COOKIE_PUT', {
            uri: uriText,
            responseHeaders: mapSummary(headers),
          });
        }
        return put.call(this, uri, headers);
      };
    });

    hook('URLConnection request properties', () => {
      const UC = Java.use('java.net.URLConnection');
      const set = UC.setRequestProperty.overload('java.lang.String', 'java.lang.String');
      set.implementation = function (name, value) {
        const url = urlOfConnection(this);
        if (interesting(url) || interesting(name) || interesting(value)) {
          emit('SET_REQUEST_PROPERTY', { url, name: s(name), value: s(value) });
        }
        return set.call(this, name, value);
      };
      const add = UC.addRequestProperty.overload('java.lang.String', 'java.lang.String');
      add.implementation = function (name, value) {
        const url = urlOfConnection(this);
        if (interesting(url) || interesting(name) || interesting(value)) {
          emit('ADD_REQUEST_PROPERTY', { url, name: s(name), value: s(value) });
        }
        return add.call(this, name, value);
      };
    });

    hook('StringBody write', () => {
      const JString = Java.use('java.lang.String');
      const Body = Java.use('org.xutils.http.body.StringBody');
      const writeTo = Body.writeTo.overload('java.io.OutputStream');
      writeTo.implementation = function (out) {
        const body = safeCall(() => s(JString.$new(this.a.value, 'UTF-8')), '<body unavailable>');
        if (interesting(body)) {
          emit('STRING_BODY_WRITE', {
            contentType: safeCall(() => s(this.getContentType()), ''),
            length: safeCall(() => this.a.value.length, 0),
            body,
          });
        }
        return writeTo.call(this, out);
      };
    });

    emit('READY', 'focused HTTP hooks installed; auto-trigger scheduled');
    triggerMiniSession();
  });
}

if (typeof Java === 'undefined' || !Java.available) {
  console.log('[http-final] Java is not available in this Frida realm');
} else {
  install();
}
