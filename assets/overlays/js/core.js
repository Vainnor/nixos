// Shared code for every scene: settings, chat captions, events, uptime, crawl, meter, debug panel.
(function () {
  var defaults = {
    channel: "vainnor", minutes: 5, endHold: 20, nowPlaying: "", discord: "", startTime: "",
    chatMax: 6, chatTtl: 60, hideUsers: [], seChannelId: "", seJwt: "", debug: false,
    camera: true, cameraSource: "", obsUrl: "ws://127.0.0.1:4455", obsPassword: "", gameplayCrawl: false
  };
  var cfg = Object.assign({}, defaults, window.VAINNOR || {}, window.VAINNOR_PAGE || {});
  var q = new URLSearchParams(location.search);
  q.forEach(function (v, k) {
    if (!(k in defaults)) return;
    var d = defaults[k];
    cfg[k] = typeof d === "number" ? parseFloat(v) : typeof d === "boolean" ? v === "1" || v === "true" : v;
  });

  // ---------- tiny event bus ----------
  var handlers = {};
  var bus = {
    on: function (t, fn) { (handlers[t] = handlers[t] || []).push(fn); },
    emit: function (t, d) { (handlers[t] || []).forEach(function (fn) { try { fn(d); } catch (e) { console.error(e); } }); }
  };

  function pad(n) { return (n < 10 ? "0" : "") + n; }
  function clock(ms) {
    var s = Math.max(0, Math.floor(ms / 1000));
    return pad(Math.floor(s / 3600)) + ":" + pad(Math.floor(s / 60) % 60) + ":" + pad(s % 60);
  }

  // ---------- signal meter: five bars ----------
  function meter(el) {
    var bars = Array.prototype.slice.call(el.querySelectorAll("i"));
    var level = 5, dipTimer = null;
    function paint(n) { bars.forEach(function (b, i) { b.classList.toggle("on", i < n); b.classList.remove("seek"); }); }
    var api = {
      set: function (n) { level = n; paint(n); },
      seek: function (n) { if (bars[n]) bars[n].classList.add("seek"); },
      dip: function (ms) {
        paint(1);
        clearTimeout(dipTimer);
        dipTimer = setTimeout(function () { paint(level); }, ms || 1200);
      }
    };
    api.set(5);
    return api;
  }

  // ---------- uptime: time on air ----------
  // Uses cfg.startTime if set. Otherwise remembers the first load and restarts after 8 hours without a load.
  function uptime(el) {
    var start = null, key = "vainnor.startedAt", seenKey = "vainnor.lastSeen";
    function resolve() {
      if (cfg.startTime) { var t = Date.parse(cfg.startTime); if (isFinite(t)) return t; }
      try {
        var s = parseInt(localStorage.getItem(key), 10), seen = parseInt(localStorage.getItem(seenKey), 10);
        if (!s || !seen || Date.now() - seen > 8 * 3600 * 1000) { s = Date.now(); localStorage.setItem(key, s); }
        localStorage.setItem(seenKey, Date.now());
        return s;
      } catch (e) { return Date.now(); }
    }
    start = resolve();
    function tick() {
      el.textContent = clock(Date.now() - start);
      try { localStorage.setItem(seenKey, Date.now()); } catch (e) {}
    }
    tick(); setInterval(tick, 1000);
    return { reset: function () { try { localStorage.removeItem(key); } catch (e) {} start = Date.now(); try { localStorage.setItem(key, start); } catch (e) {} tick(); } };
  }

  // ---------- station crawl ----------
  function crawl(el) {
    var items = [];
    if (cfg.nowPlaying) items.push("Now playing: " + cfg.nowPlaying);
    items.push("twitch.tv/" + cfg.channel);
    if (cfg.discord) items.push(cfg.discord);
    var text = items.join("   ×   ") + "   ×   ";
    var track = document.createElement("div");
    track.className = "crawl-track";
    for (var i = 0; i < 8; i++) { var s = document.createElement("span"); s.textContent = text; track.appendChild(s); }
    el.appendChild(track);
    var w = track.scrollWidth / 2;
    track.style.animationDuration = Math.max(20, w / 80) + "s";
  }

  // ---------- chat captions: anonymous read-only Twitch chat ----------
  function chat(box) {
    var ch = String(cfg.channel || "").toLowerCase().replace(/^#/, "");
    if (!ch) return;
    var hidden = (cfg.hideUsers || []).map(function (u) { return String(u).toLowerCase(); });

    function add(name, text) {
      var cap = document.createElement("div");
      cap.className = "cap flick";
      var n = document.createElement("b"); n.textContent = name;
      var t = document.createElement("span"); t.textContent = text;
      cap.appendChild(n); cap.appendChild(t);
      box.appendChild(cap);
      while (box.children.length > cfg.chatMax) box.removeChild(box.firstChild);
      setTimeout(function () { cap.classList.add("out"); setTimeout(function () { if (cap.parentNode) cap.parentNode.removeChild(cap); }, 700); }, cfg.chatTtl * 1000);
      bus.emit("chat", { name: name, text: text });
    }
    bus.on("fake-chat", function (m) { add(m.name, m.text); });

    function stripEmotes(text, tag) {
      if (!tag) return text;
      var ranges = [];
      tag.split("/").forEach(function (e) {
        var parts = e.split(":"); if (parts.length < 2) return;
        parts[1].split(",").forEach(function (r) { var ab = r.split("-"); ranges.push([parseInt(ab[0], 10), parseInt(ab[1], 10)]); });
      });
      ranges.sort(function (a, b) { return b[0] - a[0]; });
      var chars = Array.from(text);
      ranges.forEach(function (r) { chars.splice(r[0], r[1] - r[0] + 1); });
      return chars.join("").replace(/\s+/g, " ").trim();
    }

    var ws, retry = 1000;
    function open() {
      ws = new WebSocket("wss://irc-ws.chat.twitch.tv:443");
      ws.onopen = function () {
        retry = 1000;
        ws.send("CAP REQ :twitch.tv/tags");
        ws.send("PASS SCHMOOPIIE");
        ws.send("NICK justinfan" + Math.floor(10000 + Math.random() * 80000));
        ws.send("JOIN #" + ch);
      };
      ws.onmessage = function (ev) {
        ev.data.split("\r\n").forEach(function (line) {
          if (!line) return;
          if (line.indexOf("PING") === 0) { ws.send("PONG :tmi.twitch.tv"); return; }
          var m = line.match(/^@(\S+) :([^!]+)![^ ]+ PRIVMSG #\S+ :(.*)$/);
          if (!m) return;
          var tags = {};
          m[1].split(";").forEach(function (kv) { var i = kv.indexOf("="); tags[kv.slice(0, i)] = kv.slice(i + 1); });
          var name = (tags["display-name"] || m[2]).replace(/\\s/g, " ");
          if (hidden.indexOf(name.toLowerCase()) !== -1) return;
          var text = m[3];
          if (text.charCodeAt(0) === 1) text = text.replace(/^\u0001ACTION /, "").replace(/\u0001$/, "");
          if (text.charAt(0) === "!") return;
          text = stripEmotes(text, tags.emotes);
          if (text) add(name, text);
        });
      };
      ws.onclose = function () { setTimeout(open, retry); retry = Math.min(retry * 2, 15000); };
      ws.onerror = function () { try { ws.close(); } catch (e) {} };
    }
    open();
  }

  // ---------- events: StreamElements Astro socket ----------
  function normalise(a) {
    var d = (a && a.data) || {}, type = a && a.type;
    var name = d.displayName || d.username || "someone";
    var amount = d.amount != null ? d.amount : (d.viewers != null ? d.viewers : (d.bits != null ? d.bits : null));
    if (type === "follow" || type === "follower") return { type: "follow", name: name };
    if (type === "subscriber" || type === "sponsor") return { type: "sub", name: name, amount: amount };
    if (type === "raid") return { type: "raid", name: name, amount: amount };
    if (type === "cheer") return { type: "cheer", name: name, amount: amount };
    return null;
  }
  function events() {
    if (!cfg.seJwt || !cfg.seChannelId) return;
    var ws, retry = 1000;
    function open() {
      ws = new WebSocket("wss://astro.streamelements.com/");
      ws.onmessage = function (ev) {
        var msg; try { msg = JSON.parse(ev.data); } catch (e) { return; }
        if (msg.type === "welcome") {
          ws.send(JSON.stringify({ type: "subscribe", nonce: "vainnor-1", data: { topic: "channel.activities", room: cfg.seChannelId, token: cfg.seJwt, token_type: "jwt" } }));
          retry = 1000;
        } else if (msg.type === "message" && msg.topic === "channel.activities") {
          var e = normalise(msg.data);
          if (e) bus.emit("alert", e);
        } else if (msg.type === "response" && msg.error) {
          console.error("StreamElements:", msg.error, msg.data && msg.data.message);
        }
      };
      ws.onclose = function () { setTimeout(open, retry); retry = Math.min(retry * 2, 15000); };
      ws.onerror = function () { try { ws.close(); } catch (e) {} };
    }
    open();
  }

  // ---------- webcam toggle: follows the webcam source in OBS ----------
  // apply(true|false) is called whenever the camera should show or hide.
  // With cameraSource set, it watches OBS (obs-websocket v5, built into OBS 28+).
  // Without it, it uses cfg.camera and the debug button.
  function camera(apply) {
    var manual = !!cfg.camera;
    apply(manual);
    var api = { toggle: function () { manual = !manual; apply(manual); } };
    if (!cfg.cameraSource) return api;

    var ws, retry = 1000, n = 0, pending = {}, timer = null;
    function sha(str) {
      return crypto.subtle.digest("SHA-256", new TextEncoder().encode(str)).then(function (b) {
        return btoa(String.fromCharCode.apply(null, new Uint8Array(b)));
      });
    }
    function req(type, data) {
      return new Promise(function (res) {
        var id = "r" + (++n);
        pending[id] = res;
        ws.send(JSON.stringify({ op: 6, d: { requestType: type, requestId: id, requestData: data || {} } }));
      });
    }
    function visible(scene, depth) {
      return req("GetSceneItemList", { sceneName: scene }).then(function (r) {
        var items = (r.responseData && r.responseData.sceneItems) || [];
        for (var i = 0; i < items.length; i++) if (items[i].sourceName === cfg.cameraSource) return !!items[i].sceneItemEnabled;
        var chain = Promise.resolve(null);
        items.forEach(function (it) {
          if (depth < 3 && it.sceneItemEnabled && it.sourceType === "OBS_SOURCE_TYPE_SCENE") {
            chain = chain.then(function (found) { return found !== null ? found : visible(it.sourceName, depth + 1); });
          }
        });
        return chain;
      });
    }
    function evaluate() {
      req("GetCurrentProgramScene").then(function (r) {
        var d = r.responseData || {};
        return visible(d.currentProgramSceneName || d.sceneName, 0);
      }).then(function (v) {
        if (v === null) { console.warn("Webcam source '" + cfg.cameraSource + "' not found in the current scene."); v = true; }
        apply(v);
      });
    }
    function open() {
      ws = new WebSocket(cfg.obsUrl);
      ws.onmessage = function (ev) {
        var m; try { m = JSON.parse(ev.data); } catch (e) { return; }
        if (m.op === 0) {
          var ident = { rpcVersion: 1, eventSubscriptions: 4 | 128 };   // Scenes + SceneItems
          var a = m.d && m.d.authentication;
          if (a) {
            sha(cfg.obsPassword + a.salt).then(function (secret) { return sha(secret + a.challenge); }).then(function (auth) {
              ident.authentication = auth;
              ws.send(JSON.stringify({ op: 1, d: ident }));
            });
          } else ws.send(JSON.stringify({ op: 1, d: ident }));
        } else if (m.op === 2) {
          retry = 1000; evaluate();
        } else if (m.op === 5) {
          var t = m.d && m.d.eventType;
          if (t === "SceneItemEnableStateChanged" || t === "CurrentProgramSceneChanged" || t === "SceneItemCreated" || t === "SceneItemRemoved") {
            clearTimeout(timer); timer = setTimeout(evaluate, 80);
          }
        } else if (m.op === 7) {
          var cb = pending[m.d.requestId]; if (cb) { delete pending[m.d.requestId]; cb(m.d); }
        }
      };
      ws.onclose = function () { setTimeout(open, retry); retry = Math.min(retry * 2, 15000); };
      ws.onerror = function () { try { ws.close(); } catch (e) {} };
    }
    open();
    return api;
  }

  // ---------- debug panel ----------
  function debug(extra) {
    if (!cfg.debug) return;
    var p = document.createElement("div");
    p.className = "debug";
    function btn(label, fn) { var b = document.createElement("button"); b.textContent = label; b.onclick = fn; p.appendChild(b); }
    btn("Follow", function () { bus.emit("alert", { type: "follow", name: "TestViewer" }); });
    btn("Sub", function () { bus.emit("alert", { type: "sub", name: "TestViewer", amount: 3 }); });
    btn("Raid", function () { bus.emit("alert", { type: "raid", name: "TestRaider", amount: 24 }); });
    btn("Cheer", function () { bus.emit("alert", { type: "cheer", name: "TestViewer", amount: 100 }); });
    btn("Chat", function () { bus.emit("fake-chat", { name: "TestViewer", text: "this is a test message from the debug panel" }); });
    (extra || []).forEach(function (x) { btn(x.label, x.fn); });
    document.body.appendChild(p);
  }

  window.Signal = { cfg: cfg, bus: bus, meter: meter, uptime: uptime, crawl: crawl, chat: chat, events: events, camera: camera, debug: debug, clock: clock, pad: pad };
})();
