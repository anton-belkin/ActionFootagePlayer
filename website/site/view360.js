// The 360 tile: a 360 clip (2:1, the whole sphere unrolled) shown as a view you turn by dragging.
// With no clip to load it draws a plain sky-and-sea stand-in, so the tile still turns.
(function () {
  var box = document.getElementById("view360"); if (!box) return;
  var canvas = box.querySelector("canvas"), gl = canvas.getContext("webgl"); if (!gl) return;

  function shader(type, src) { var s = gl.createShader(type); gl.shaderSource(s, src); gl.compileShader(s); return s; }
  var prog = gl.createProgram();
  gl.attachShader(prog, shader(gl.VERTEX_SHADER, "attribute vec2 a; void main(){ gl_Position = vec4(a, 0., 1.); }"));
  gl.attachShader(prog, shader(gl.FRAGMENT_SHADER,
    "precision highp float; uniform sampler2D t; uniform vec2 r; uniform float yaw, pitch, f;" +
    "void main(){ vec2 p = (gl_FragCoord.xy * 2. - r) / r.y; vec3 d = normalize(vec3(p * f, -1.));" +
    "float cp = cos(pitch), sp = sin(pitch); d = vec3(d.x, d.y * cp - d.z * sp, d.y * sp + d.z * cp);" +
    "float cy = cos(yaw), sy = sin(yaw); d = vec3(d.x * cy - d.z * sy, d.y, d.x * sy + d.z * cy);" +
    "gl_FragColor = texture2D(t, vec2(atan(d.x, -d.z) / 6.2831853 + .5, .5 - asin(clamp(d.y, -1., 1.)) / 3.14159265)); }"));
  gl.linkProgram(prog); gl.useProgram(prog);
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0); gl.bindAttribLocation(prog, 0, "a"); gl.linkProgram(prog); gl.useProgram(prog);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  gl.bindTexture(gl.TEXTURE_2D, gl.createTexture());
  [[gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE], [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE], [gl.TEXTURE_MIN_FILTER, gl.LINEAR], [gl.TEXTURE_MAG_FILTER, gl.LINEAR]]
    .forEach(function (p) { gl.texParameteri(gl.TEXTURE_2D, p[0], p[1]); });
  var U = {}; ["r", "yaw", "pitch", "f"].forEach(function (n) { U[n] = gl.getUniformLocation(prog, n); });

  function standIn() {
    var c = document.createElement("canvas"); c.width = 2048; c.height = 1024; var x = c.getContext("2d");
    var sky = x.createLinearGradient(0, 0, 0, 512); sky.addColorStop(0, "#1d5fa8"); sky.addColorStop(.75, "#8fd0ee"); sky.addColorStop(1, "#ffe2b0");
    x.fillStyle = sky; x.fillRect(0, 0, 2048, 512);
    var sea = x.createLinearGradient(0, 512, 0, 1024); sea.addColorStop(0, "#0a87a6"); sea.addColorStop(1, "#012a4d");
    x.fillStyle = sea; x.fillRect(0, 512, 2048, 512);
    var sun = x.createRadialGradient(1024, 430, 0, 1024, 430, 150); sun.addColorStop(0, "#fff"); sun.addColorStop(.25, "#ffd66b"); sun.addColorStop(1, "rgba(255,154,60,0)");
    x.fillStyle = sun; x.fillRect(824, 230, 400, 400);
    x.fillStyle = "rgba(255,255,255,.85)"; x.font = "600 44px system-ui, sans-serif"; x.textAlign = "center";
    ["S", "W", "N", "E"].forEach(function (l, i) { x.fillText(l, i * 512 + (i ? 0 : 24), 500); });
    return c;
  }
  var source = standIn(), live = false, video = document.createElement("video");
  video.muted = true; video.loop = true; video.playsInline = true; video.preload = "metadata";
  video.addEventListener("playing", function () { source = video; live = true; });
  if (box.dataset.src) { video.src = box.dataset.src; }

  var yaw0 = (+box.dataset.yaw || 0) * Math.PI / 180, yaw = yaw0, pitch = 0, fov = 100, grabbed = false, last, seen = false, t0 = 0, dirty = true;
  var still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  box.addEventListener("pointerdown", function (e) { box.setPointerCapture(e.pointerId); box.classList.add("dragging"); grabbed = true; last = e; });
  box.addEventListener("pointermove", function (e) {
    if (!box.hasPointerCapture(e.pointerId)) return;
    var k = 1.6 / box.clientHeight; yaw -= (e.clientX - last.clientX) * k;
    pitch = Math.max(-1.3, Math.min(1.3, pitch + (e.clientY - last.clientY) * k)); last = e; dirty = true;
  });
  ["pointerup", "pointercancel"].forEach(function (n) { box.addEventListener(n, function () { box.classList.remove("dragging"); }); });
  // The drawn panel's two live controls: the field of view and "Look ahead again"
  var slider = document.getElementById("fov360"), shown = document.getElementById("fov360-value"), again = document.getElementById("reset360");
  if (slider) slider.addEventListener("input", function () { fov = +slider.value; if (shown) shown.textContent = fov + "°"; dirty = true; });
  if (again) again.addEventListener("click", function () { yaw = yaw0; pitch = 0; grabbed = true; dirty = true; });
  new IntersectionObserver(function (es) {
    seen = es[0].isIntersecting;
    if (seen && box.dataset.src && video.paused) video.play().catch(function () {}); else if (!seen) video.pause();
  }).observe(box);

  (function frame(t) {
    requestAnimationFrame(frame);
    var dt = (t - t0) / 1000 || 0; t0 = t; if (!seen) return;
    if (!grabbed && !still) { yaw += dt * .06; dirty = true; }
    var w = Math.round(box.clientWidth * Math.min(2, devicePixelRatio)), h = Math.round(box.clientHeight * Math.min(2, devicePixelRatio));
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; gl.viewport(0, 0, w, h); dirty = true; }
    if (!dirty && !live) return; dirty = false;
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB, gl.RGB, gl.UNSIGNED_BYTE, source);
    gl.uniform2f(U.r, w, h); gl.uniform1f(U.yaw, yaw); gl.uniform1f(U.pitch, pitch); gl.uniform1f(U.f, Math.tan(fov * Math.PI / 360) * h / w);
    if (live && video.duration) box.style.setProperty("--prog", (video.currentTime / video.duration * 100) + "%");
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  })(0);
})();
