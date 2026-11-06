// Starting Soon: countdown and signal meter.
// Length comes from js/config.js (minutes). The timer starts when the page loads.
// In OBS, tick "Refresh browser when scene becomes active" so it restarts each time you switch to this scene.
(function () {
  var minutes = Signal.cfg.minutes;
  if (!isFinite(minutes) || minutes < 0) minutes = 5;

  var total = minutes * 60 * 1000;
  var endAt = Date.now() + total;

  var stage = document.getElementById("stage");
  var plate = document.getElementById("plate");
  var stateEl = document.getElementById("state");
  var timeEl = document.getElementById("time");
  var bars = Array.prototype.slice.call(document.querySelectorAll("#meter i"));

  function pad(n) { return (n < 10 ? "0" : "") + n; }

  function tick() {
    var remaining = Math.max(0, endAt - Date.now());
    var secs = Math.ceil(remaining / 1000);
    timeEl.textContent = pad(Math.floor(secs / 60)) + ":" + pad(secs % 60);

    var progress = total === 0 ? 1 : 1 - remaining / total;
    var lit = remaining === 0 ? 5 : 1 + Math.floor(progress * 4);

    bars.forEach(function (bar, i) {
      bar.classList.toggle("on", i < lit);
      bar.classList.toggle("seek", remaining > 0 && i === lit);
    });

    if (remaining === 0) {
      plate.classList.add("live");
      stateEl.textContent = stateEl.getAttribute("data-onair") || "On air";
      stage.classList.remove("weak");
      stage.classList.add("onair");
    } else {
      stage.classList.toggle("weak", progress < 0.4);
    }
  }

  tick();
  setInterval(tick, 250);
})();
