(function () {
  var viewer = document.getElementById('viewer');
  var cLog = document.getElementById('cLog');
  var cHdr = document.getElementById('cHdr');
  var range = document.getElementById('split');
  if (!viewer || !cLog || !cHdr || !range) return;

  function setSplit(v) { viewer.style.setProperty('--split', v + '%'); }
  range.addEventListener('input', function () { setSplit(range.value); });
  setSplit(range.value);

  // Deterministic pseudo-random so the scene is the same on every load
  function rng(seed) { return function () { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296; }; }

  function drawScene(ctx, w, h) {
    var r = rng(7);
    // Water column
    var g = ctx.createLinearGradient(0, 0, 0, h);
    g.addColorStop(0, '#7FD3E3');
    g.addColorStop(0.12, '#43A9C9');
    g.addColorStop(0.45, '#17698F');
    g.addColorStop(0.8, '#0A3B58');
    g.addColorStop(1, '#062638');
    ctx.fillStyle = g; ctx.fillRect(0, 0, w, h);

    // Sun through the surface
    var sx = w * 0.64, sy = -h * 0.08;
    var sun = ctx.createRadialGradient(sx, sy, 0, sx, sy, w * 0.55);
    sun.addColorStop(0, 'rgba(255,255,250,0.95)');
    sun.addColorStop(0.18, 'rgba(230,250,255,0.55)');
    sun.addColorStop(1, 'rgba(230,250,255,0)');
    ctx.fillStyle = sun; ctx.fillRect(0, 0, w, h);

    // Light rays
    ctx.save();
    ctx.globalCompositeOperation = 'lighter';
    for (var i = 0; i < 9; i++) {
      var a = -0.55 + i * 0.13 + r() * 0.05;
      var spread = 0.02 + r() * 0.035;
      var len = h * (1.0 + r() * 0.3);
      var rg = ctx.createLinearGradient(sx, sy, sx + Math.sin(a) * len, sy + Math.cos(a) * len);
      rg.addColorStop(0, 'rgba(210,245,255,' + (0.20 + r() * 0.12) + ')');
      rg.addColorStop(1, 'rgba(210,245,255,0)');
      ctx.fillStyle = rg;
      ctx.beginPath();
      ctx.moveTo(sx, sy);
      ctx.lineTo(sx + Math.sin(a - spread) * len, sy + Math.cos(a - spread) * len);
      ctx.lineTo(sx + Math.sin(a + spread) * len, sy + Math.cos(a + spread) * len);
      ctx.closePath(); ctx.fill();
    }
    ctx.restore();

    // Surface ripples
    ctx.save();
    ctx.strokeStyle = 'rgba(255,255,255,0.22)';
    ctx.lineWidth = Math.max(1, w / 700);
    for (var k = 0; k < 5; k++) {
      ctx.beginPath();
      var y0 = h * (0.015 + k * 0.018);
      for (var x = 0; x <= w; x += w / 60) {
        var y = y0 + Math.sin(x / w * 18 + k * 1.7) * h * 0.006;
        if (x === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      }
      ctx.stroke();
    }
    ctx.restore();

    // Fish school
    ctx.save();
    for (var f = 0; f < 34; f++) {
      var fx = w * (0.16 + r() * 0.28), fy = h * (0.36 + r() * 0.16);
      var fs = w * (0.009 + r() * 0.006);
      ctx.fillStyle = 'rgba(8,40,58,' + (0.55 + r() * 0.3) + ')';
      ctx.beginPath();
      ctx.ellipse(fx, fy, fs * 1.6, fs * 0.6, -0.12, 0, Math.PI * 2);
      ctx.moveTo(fx - fs * 1.4, fy);
      ctx.lineTo(fx - fs * 2.4, fy - fs * 0.7);
      ctx.lineTo(fx - fs * 2.4, fy + fs * 0.7);
      ctx.closePath();
      ctx.fill();
    }
    ctx.restore();

    // Sand floor
    var floorY = h * 0.8;
    var sand = ctx.createLinearGradient(0, floorY - h * 0.05, 0, h);
    sand.addColorStop(0, '#6E9C9A');
    sand.addColorStop(1, '#203E47');
    ctx.fillStyle = sand;
    ctx.beginPath();
    ctx.moveTo(0, h);
    ctx.lineTo(0, floorY + h * 0.02);
    for (var sxp = 0; sxp <= w; sxp += w / 40) {
      ctx.lineTo(sxp, floorY + Math.sin(sxp / w * 7) * h * 0.022 + Math.sin(sxp / w * 23) * h * 0.006);
    }
    ctx.lineTo(w, h); ctx.closePath(); ctx.fill();

    // Reef: rocks and corals
    function blob(cx, cy, rx, ry, color) {
      ctx.fillStyle = color;
      ctx.beginPath();
      for (var t = 0; t <= Math.PI * 2 + 0.01; t += Math.PI / 14) {
        var rr = 1 + (r() - 0.5) * 0.25;
        var px = cx + Math.cos(t) * rx * rr, py = cy + Math.sin(t) * ry * rr;
        if (t === 0) ctx.moveTo(px, py); else ctx.lineTo(px, py);
      }
      ctx.closePath(); ctx.fill();
    }
    blob(w * 0.82, h * 0.86, w * 0.16, h * 0.13, '#2A4F57');
    blob(w * 0.07, h * 0.88, w * 0.12, h * 0.1, '#25474F');
    blob(w * 0.78, h * 0.76, w * 0.05, h * 0.06, '#D9603A');
    blob(w * 0.87, h * 0.74, w * 0.035, h * 0.075, '#8D4FA0');
    blob(w * 0.12, h * 0.79, w * 0.045, h * 0.05, '#E2B23A');
    blob(w * 0.69, h * 0.83, w * 0.03, h * 0.035, '#E07B4A');

    // Branching coral
    ctx.save();
    ctx.strokeStyle = '#E9874F';
    ctx.lineCap = 'round';
    function branch(x, y, len, ang, depth) {
      if (depth === 0) return;
      var x2 = x + Math.sin(ang) * len, y2 = y - Math.cos(ang) * len;
      ctx.lineWidth = Math.max(1, depth * w / 900);
      ctx.beginPath(); ctx.moveTo(x, y); ctx.lineTo(x2, y2); ctx.stroke();
      branch(x2, y2, len * 0.72, ang - 0.45 - r() * 0.2, depth - 1);
      branch(x2, y2, len * 0.72, ang + 0.4 + r() * 0.2, depth - 1);
    }
    branch(w * 0.92, h * 0.8, h * 0.08, -0.1, 5);
    branch(w * 0.2, h * 0.82, h * 0.06, 0.15, 4);
    ctx.restore();

    // Particles
    for (var p = 0; p < 160; p++) {
      ctx.fillStyle = 'rgba(220,245,250,' + (0.12 + r() * 0.35) + ')';
      ctx.beginPath();
      ctx.arc(r() * w, r() * h * 0.85, Math.max(0.6, r() * w / 650), 0, Math.PI * 2);
      ctx.fill();
    }
  }

  // Sunset over the sea: a high-contrast scene for the hub page
  function drawSunset(ctx, w, h) {
    var r = rng(11);
    var horizon = h * 0.6;
    var sky = ctx.createLinearGradient(0, 0, 0, horizon);
    sky.addColorStop(0, '#1E2F5C');
    sky.addColorStop(0.45, '#5E4478');
    sky.addColorStop(0.78, '#C8663E');
    sky.addColorStop(1, '#E8A456');
    ctx.fillStyle = sky; ctx.fillRect(0, 0, w, horizon);

    // Clouds, lit from below
    for (var i = 0; i < 9; i++) {
      var cy = h * (0.08 + r() * 0.42), cx = w * r(), cw = w * (0.2 + r() * 0.3), ch = h * (0.012 + r() * 0.02);
      var cg = ctx.createLinearGradient(0, cy - ch, 0, cy + ch);
      cg.addColorStop(0, 'rgba(60,36,80,0.6)');
      cg.addColorStop(1, 'rgba(240,150,100,0.55)');
      ctx.fillStyle = cg;
      ctx.beginPath(); ctx.ellipse(cx, cy, cw / 2, ch, 0, 0, Math.PI * 2); ctx.fill();
    }

    // Sea
    var sea = ctx.createLinearGradient(0, horizon, 0, h);
    sea.addColorStop(0, '#5A4560');
    sea.addColorStop(0.35, '#2E3654');
    sea.addColorStop(1, '#141C33');
    ctx.fillStyle = sea; ctx.fillRect(0, horizon, w, h - horizon);

    // Sun and glow, kept below clipping so the full-range side has color in it
    var sx = w * 0.42, sy = horizon - h * 0.07;
    var glow = ctx.createRadialGradient(sx, sy, 0, sx, sy, w * 0.5);
    glow.addColorStop(0, 'rgba(255,236,190,0.85)');
    glow.addColorStop(0.08, 'rgba(255,196,120,0.5)');
    glow.addColorStop(0.3, 'rgba(255,140,70,0.2)');
    glow.addColorStop(1, 'rgba(255,140,70,0)');
    ctx.fillStyle = glow; ctx.fillRect(0, 0, w, h);
    ctx.fillStyle = '#FFF6E0';
    ctx.beginPath(); ctx.arc(sx, sy, h * 0.045, 0, Math.PI * 2); ctx.fill();

    // Headland on the right: dim but textured
    var hx = w * 0.56;
    var hg = ctx.createLinearGradient(0, horizon - h * 0.14, 0, horizon);
    hg.addColorStop(0, '#45395A'); hg.addColorStop(1, '#262539');
    ctx.fillStyle = hg;
    ctx.beginPath(); ctx.moveTo(hx, horizon + 2);
    for (var x = hx; x <= w + 1; x += w / 80) {
      var t = (x - hx) / (w - hx);
      var y = horizon - h * 0.14 * Math.pow(Math.sin(Math.PI * Math.min(1, t * 0.75 + 0.05)), 0.6) * (0.85 + 0.15 * Math.sin(x / w * 40));
      ctx.lineTo(x, y);
    }
    ctx.lineTo(w, horizon + 2); ctx.closePath(); ctx.fill();
    ctx.save(); ctx.clip();
    for (var q = 0; q < 90; q++) {
      var tx = hx + r() * (w - hx), ty = horizon - r() * h * 0.13;
      ctx.fillStyle = r() > 0.5 ? 'rgba(22,22,36,0.55)' : 'rgba(110,80,110,0.35)';
      ctx.beginPath(); ctx.ellipse(tx, ty, w * (0.006 + r() * 0.012), h * (0.004 + r() * 0.01), 0, 0, Math.PI * 2); ctx.fill();
    }
    ctx.restore();

    // Glitter on the water
    ctx.save();
    ctx.globalCompositeOperation = 'lighter';
    for (var k = 0; k < 300; k++) {
      var u = r();
      var yy = horizon + Math.pow(u, 1.6) * (h - horizon);
      var spread = w * (0.01 + 0.17 * Math.pow(u, 1.2));
      var xx = sx + (r() - 0.5) * 2 * spread;
      var len = w * (0.004 + 0.02 * u * r());
      ctx.fillStyle = 'rgba(255,200,140,' + (0.2 + 0.4 * (1 - u) * r()) + ')';
      ctx.fillRect(xx - len / 2, yy, len, Math.max(1, h * 0.002));
    }
    ctx.restore();

    // Low rocks along the whole foreground, rim-lit by the sun
    var rg = ctx.createLinearGradient(0, h * 0.84, 0, h);
    rg.addColorStop(0, '#3E3248'); rg.addColorStop(1, '#1E1F30');
    ctx.fillStyle = rg;
    ctx.beginPath(); ctx.moveTo(0, h);
    var pts = [];
    for (var px = 0; px <= w + 1; px += w / 60) {
      var py = h * (0.9 - 0.05 * Math.sin(px / w * 7 + 1) - 0.03 * Math.sin(px / w * 19));
      pts.push([px, py]); ctx.lineTo(px, py);
    }
    ctx.lineTo(w, h); ctx.closePath(); ctx.fill();
    ctx.save(); ctx.clip();
    for (var m = 0; m < 80; m++) {
      ctx.fillStyle = r() > 0.5 ? 'rgba(18,18,30,0.55)' : 'rgba(130,90,100,0.3)';
      ctx.beginPath(); ctx.ellipse(w * r(), h * (0.86 + r() * 0.14), w * (0.008 + r() * 0.02), h * (0.004 + r() * 0.01), r(), 0, Math.PI * 2); ctx.fill();
    }
    ctx.restore();
    ctx.strokeStyle = 'rgba(255,170,110,0.5)'; ctx.lineWidth = Math.max(1, h * 0.004);
    ctx.beginPath(); pts.forEach(function (p, j) { j ? ctx.lineTo(p[0], p[1]) : ctx.moveTo(p[0], p[1]); }); ctx.stroke();
  }

  // Log look: lifted blacks, compressed highlights, muted color
  function toLog(d) {
    for (var i = 0; i < d.length; i += 4) {
      var R = d[i] / 255, G = d[i + 1] / 255, B = d[i + 2] / 255;
      var L = 0.2126 * R + 0.7152 * G + 0.0722 * B;
      R = L + (R - L) * 0.45; G = L + (G - L) * 0.45; B = L + (B - L) * 0.45;
      R = 0.2 + 0.58 * Math.pow(Math.max(R, 0), 0.72);
      G = 0.21 + 0.58 * Math.pow(Math.max(G, 0), 0.72);
      B = 0.2 + 0.56 * Math.pow(Math.max(B, 0), 0.72);
      d[i] = R * 255; d[i + 1] = G * 255; d[i + 2] = B * 255;
    }
  }
  // Display look: deeper shadows, brighter highlights, fuller color
  function toHdr(d) {
    for (var i = 0; i < d.length; i += 4) {
      var R = d[i] / 255, G = d[i + 1] / 255, B = d[i + 2] / 255;
      var L = 0.2126 * R + 0.7152 * G + 0.0722 * B;
      R = L + (R - L) * 1.22; G = L + (G - L) * 1.22; B = L + (B - L) * 1.22;
      var f = function (v) { v = Math.max(0, v); v = Math.pow(v, 1.12) * 1.06; return v > 0.78 ? 0.78 + (v - 0.78) * 1.35 : v; };
      d[i] = Math.min(255, f(R) * 255); d[i + 1] = Math.min(255, f(G) * 255); d[i + 2] = Math.min(255, f(B) * 255);
    }
  }

  // Camera's normal profile: highlights clip to white, shadows crush to black
  function toSdr(d) {
    for (var i = 0; i < d.length; i += 4) {
      var R = d[i] / 255, G = d[i + 1] / 255, B = d[i + 2] / 255;
      var L0 = 0.2126 * R + 0.7152 * G + 0.0722 * B;
      R = L0 + (R - L0) * 0.85; G = L0 + (G - L0) * 0.85; B = L0 + (B - L0) * 0.85;
      var f = function (v) { v = (v - 0.1) / 0.68; v = Math.min(1, Math.max(0, v)); return v * v * (3 - 2 * v) * 0.5 + v * 0.5; };
      R = f(R); G = f(G); B = f(B);
      var L = 0.2126 * R + 0.7152 * G + 0.0722 * B;
      var wash = Math.min(1, Math.max(0, (L - 0.62) / 0.38)) * 0.8;
      R += (1 - R) * wash; G += (1 - G) * wash; B += (1 - B) * wash;
      d[i] = R * 255; d[i + 1] = G * 255; d[i + 2] = B * 255;
    }
  }
  // Full range: deep but detailed shadows, rich color, highlights keep their color
  function toFull(d) {
    for (var i = 0; i < d.length; i += 4) {
      var R = d[i] / 255, G = d[i + 1] / 255, B = d[i + 2] / 255;
      var L = 0.2126 * R + 0.7152 * G + 0.0722 * B;
      R = L + (R - L) * 1.45; G = L + (G - L) * 1.45; B = L + (B - L) * 1.45;
      var f = function (v) { v = Math.max(0, v); return Math.min(1, Math.pow(v, 1.18) * 1.18); };
      d[i] = f(R) * 255; d[i + 1] = f(G) * 255; d[i + 2] = f(B) * 255;
    }
  }

  function render() {
    var rect = viewer.getBoundingClientRect();
    var dpr = Math.min(window.devicePixelRatio || 1, 2);
    var w = Math.max(320, Math.round(rect.width * dpr)), h = Math.max(180, Math.round(rect.height * dpr));
    var base = document.createElement('canvas');
    base.width = w; base.height = h;
    var bctx = base.getContext('2d');
    (viewer.dataset.scene === 'sunset' ? drawSunset : drawScene)(bctx, w, h);
    var img;
    try { img = bctx.getImageData(0, 0, w, h); } catch (e) { return; }
    var logImg = new ImageData(new Uint8ClampedArray(img.data), w, h);
    if (viewer.dataset.compare === 'sdr') { toSdr(logImg.data); toFull(img.data); }
    else { toLog(logImg.data); toHdr(img.data); }
    [cLog, cHdr].forEach(function (c) { c.width = w; c.height = h; });
    cLog.getContext('2d').putImageData(logImg, 0, 0);
    cHdr.getContext('2d').putImageData(img, 0, 0);
  }

  var t;
  window.addEventListener('resize', function () { clearTimeout(t); t = setTimeout(render, 200); });
  render();
})();
