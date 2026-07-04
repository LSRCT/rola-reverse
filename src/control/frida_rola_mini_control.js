'use strict';

const LIVE_MODEL = 'com.ebo.ebocode.deviceRolaMini.live.RolaMiniLiveModel';
const SESSION = 'j4.g';

function clampAxis(n) {
  n = Number(n);
  if (!Number.isFinite(n)) throw new Error('axis must be numeric');
  n = Math.trunc(n);
  if (n < -100) return -100;
  if (n > 100) return 100;
  return n;
}

function clampButtons(n) {
  n = Number(n);
  if (!Number.isFinite(n)) throw new Error('buttons must be numeric');
  n = Math.trunc(n);
  if (n < 0) return 0;
  if (n > 255) return 255;
  return n;
}

function chooseFirst(className, fn) {
  return new Promise((resolve, reject) => {
    Java.perform(() => {
      let matched = false;
      Java.choose(className, {
        onMatch(obj) {
          if (matched) return 'stop';
          matched = true;
          try {
            resolve(fn(obj));
          } catch (e) {
            reject(e);
          }
          return 'stop';
        },
        onComplete() {
          if (!matched) reject(new Error(className + ' not found'));
        },
      });
    });
  });
}

function sessionSnapshot(obj) {
  return {
    sid: String(obj.getSid()),
    app_rtc_uid: String(obj.getApp_rtc_uid()),
    app_rtm_uid: String(obj.getApp_rtm_uid()),
    rtc_channel: String(obj.getRtc_channel()),
    mini_rtc_uid: String(obj.getMini_rtc_uid()),
    mini_rtm_uid: String(obj.getMini_rtm_uid()),
  };
}

async function moveInternal(ly, rx, buttons) {
  const safeLy = clampAxis(ly);
  const safeRx = clampAxis(rx);
  const safeButtons = clampButtons(buttons === undefined ? 1 : buttons);
  return chooseFirst(LIVE_MODEL, model => {
    model.l(safeLy, safeRx, safeButtons);
    return {
      ok: true,
      command: { lx: 0, ly: safeLy, rx: safeRx, ry: 0, buttons: safeButtons, id: 101007 },
    };
  });
}

rpc.exports = {
  async status() {
    const out = {
      live_model: false,
      session: null,
    };

    try {
      out.session = await chooseFirst(SESSION, sessionSnapshot);
    } catch (e) {
      out.session_error = String(e);
    }

    try {
      await chooseFirst(LIVE_MODEL, () => true);
      out.live_model = true;
    } catch (e) {
      out.live_model_error = String(e);
    }

    return out;
  },

  async move(ly, rx, buttons) {
    return moveInternal(ly, rx, buttons);
  },

  async stop() {
    return moveInternal(0, 0, 1);
  },
};
