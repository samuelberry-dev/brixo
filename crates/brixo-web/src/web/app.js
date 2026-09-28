// Shared by every page: talking to the API, the banner, tab bar and footer,
// game cards, and drawing avatars.

const PALETTES = {
  skin:  [[227,185,138],[160,99,62],[242,211,176],[204,142,105],[124,78,50],[234,196,160]],
  shirt: [[196,40,28],[13,105,172],[75,151,75],[218,133,65],[107,50,124],[245,205,48],[27,42,53],[0,143,156]],
  pants: [[27,42,53],[39,70,45],[99,95,98],[105,64,40],[52,43,117],[13,105,172]],
  shoes: [[27,42,53],[27,27,27],[99,95,98],[105,64,40]],
};
const FACES = ["smile", "happy", "surprised", "determined"];
const rgb = (c) => `rgb(${c[0]},${c[1]},${c[2]})`;
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const num = (n) => Number(n || 0).toLocaleString("en-US");

async function api(path, method = "GET", body) {
  const res = await fetch(path, { method, headers: body ? {"Content-Type": "application/json"} : {},
                                  body: body ? JSON.stringify(body) : undefined });
  let data = null;
  try { data = await res.json(); } catch (e) {}
  if (!res.ok) throw new Error((data && data.error) || `something went wrong (${res.status})`);
  return data;
}

// "Sep 2026", or "" for things from before dates were kept.
function when(secs) {
  if (!secs) return "";
  return new Date(secs * 1000).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
}

const TABS = [["home", "/", "Home"], ["games", "/games", "Games"], ["avatar", "/avatar", "Character"],
              ["profile", null, "Profile"], ["friends", "/friends", "Friends"], ["learn", "/learn", "Learn"], ["download", "/download", "Get Brixo"],
              ["admin", "/admin", "Admin"]];

// Draws the banner, tab bar and footer around the page. Returns the
// logged-in user, or null.
async function shell(page) {
  let me = null;
  try { me = await api("/api/me"); } catch (e) {}
  const logo = `<a class="logo" href="/" aria-label="Brixo home">${"BRIXO".split("").map((c, i) => `<span class="b${i + 1}">${c}</span>`).join("")}</a>`;
  const user = me
    ? `<div>Hi, <a href="/users/${encodeURIComponent(me.username)}">${esc(me.username)}</a>!<br>
         <a href="#" id="logout" style="color:#cfe3f7;font-weight:normal">Log out</a></div>
       <a class="mini" href="/avatar" title="Change your character">${avatarSvg(me.avatar, true)}</a>`
    : `<form id="quicklogin" autocomplete="on">
         <input name="username" placeholder="Username" autocomplete="username" aria-label="Username">
         <input name="password" type="password" placeholder="Password" autocomplete="current-password" aria-label="Password">
         <button class="btn small green" type="submit">Log in</button>
       </form>
       <div>New here? <a href="/signup">Sign up free</a><div class="err" id="qlerr"></div></div>`;
  document.getElementById("banner").innerHTML =
    `<div class="brand">${logo}<div class="tagline">Build it.<br>Play it.<br>Share it.</div></div><div class="userbox">${user}</div>`;
  const tabs = TABS.map(([key, href, label]) => {
    if (key === "profile") {
      if (!me) return "";
      href = `/users/${encodeURIComponent(me.username)}`;
    }
    if ((key === "avatar" || key === "friends") && !me) return "";
    if (key === "friends" && me.friend_requests) label += ` <span class="badge" title="Friend requests">${me.friend_requests}</span>`;
    if (key === "admin" && !(me && me.admin)) return "";
    return `<a class="tab ${page === key ? "on" : ""}" href="${href}">${label}</a>`;
  }).join("");
  document.getElementById("nav").innerHTML = `<div class="tabs">${tabs}</div><div class="online" id="online"></div>`;
  document.getElementById("footer").innerHTML =
    `<div><a href="/">Home</a>|<a href="/games">Games</a>|<a href="/learn">Learn</a>|<a href="/download">Get Brixo</a>${me ? "" : `|<a href="/signup">Sign up</a>`}</div>
     <div style="margin-top:4px">Brixo &copy; ${new Date().getFullYear()}. Every game here was built with Brixo Studio.</div>`;
  const out = document.getElementById("logout");
  if (out) out.onclick = async (e) => { e.preventDefault(); await api("/api/logout", "POST"); location.href = "/"; };
  const ql = document.getElementById("quicklogin");
  if (ql) ql.onsubmit = async (e) => {
    e.preventDefault();
    try {
      await api("/api/login", "POST", { username: ql.username.value, password: ql.password.value });
      location.reload();
    } catch (err) { document.getElementById("qlerr").textContent = err.message; }
  };
  api("/api/stats").then((s) => {
    document.getElementById("online").innerHTML = `<span class="dot"></span>${num(s.online)} playing now`;
  }).catch(() => {});
  return me;
}

// --- games ---------------------------------------------------------------------

// A stand-in picture for games without one: sky, grass and a few bricks in
// colours picked from the game's name, so each game looks its own.
function placeholderSvg(g) {
  let h = 0;
  for (const c of g.name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  const pick = (n) => PALETTES.shirt[(h >>> n) % PALETTES.shirt.length];
  let bricks = "";
  for (let i = 0; i < 5; i++) {
    const x = 20 + ((h >>> (i * 3)) % 120) + i * 14, w = 18 + ((h >>> (i * 5)) % 22), ht = 10 + ((h >>> (i * 2)) % 26);
    bricks += `<rect x="${x}" y="${62 - ht}" width="${w}" height="${ht}" fill="${rgb(pick(i * 4))}" stroke="rgba(0,0,0,.25)"/>`;
  }
  return `<svg viewBox="0 0 160 90" xmlns="http://www.w3.org/2000/svg" preserveAspectRatio="xMidYMid slice">
    <defs><linearGradient id="sk${g.id}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#5da0e0"/><stop offset="1" stop-color="#cfe6fb"/></linearGradient></defs>
    <rect width="160" height="90" fill="url(#sk${g.id})"/><rect y="62" width="160" height="28" fill="#4b974b"/>${bricks}
    <text x="80" y="82" text-anchor="middle" font-family="Arial Black, Arial" font-size="11" fill="#fff" stroke="rgba(0,0,0,.35)" stroke-width=".6">${esc(g.name.toUpperCase().slice(0, 22))}</text></svg>`;
}

function thumb(g) {
  return g.has_thumbnail ? `<img src="/api/games/${g.id}/thumbnail" alt="" loading="lazy">` : placeholderSvg(g);
}

function gameCard(g) {
  const url = `/games/${g.id}`;
  return `<div class="card">
    <a class="thumb" href="${url}">${thumb(g)}</a>
    <a class="name" href="${url}" title="${esc(g.name)}">${esc(g.name)}</a>
    <div class="by">by <a href="/users/${encodeURIComponent(g.owner)}">${esc(g.owner)}</a></div>
    <div class="meta">${g.playing ? `<b>${num(g.playing)} playing</b> &middot; ` : ""}${num(g.visits)} visits</div>
  </div>`;
}

// Most played first: people playing now, then all-time visits.
const popular = (games) => [...games].sort((a, b) => b.playing - a.playing || b.visits - a.visits || a.id - b.id);

// Pressing Play: get a ticket, open Brixo Player through its link, and
// show "Starting Brixo Player..." with a download button, like Roblox. The
// box closes itself once Brixo Player takes over the screen.
async function playGame(g, button) {
  button.disabled = true;
  try {
    const pass = await api(`/api/games/${g.id}/play`, "POST");
    const box = startingBox(g);
    const gone = () => { box.remove(); window.removeEventListener("blur", gone); };
    window.addEventListener("blur", gone);
    location.href = `brixo://play?server=${encodeURIComponent(pass.server)}&ticket=${encodeURIComponent(pass.ticket)}` +
                    `&game=${encodeURIComponent(g.name)}`;
  } catch (e) { alert(e.message); }
  setTimeout(() => (button.disabled = false), 2000);
}

function startingBox(g) {
  const bg = document.createElement("div");
  bg.className = "modal-bg";
  bg.innerHTML = `<div class="modal" role="dialog" aria-label="Starting Brixo Player">
      <h2>Starting Brixo Player...</h2>
      <div class="inner">
        <div class="spinner"><span></span><span></span><span></span></div>
        <p>Joining <b>${esc(g.name)}</b>. If your browser asks, choose <b>Open Brixo Player</b>.</p>
        <hr>
        <p><b>Nothing happening?</b> You need Brixo Player to play.</p>
        <a class="btn green big" href="/download">Download Brixo Player</a>
        <p class="hint" style="margin-top:8px"><a href="#" data-close>Close</a></p>
      </div></div>`;
  bg.addEventListener("click", (e) => {
    if (e.target === bg || e.target.hasAttribute("data-close")) { e.preventDefault(); bg.remove(); }
  });
  document.body.appendChild(bg);
  return bg;
}

// "31.2 MB"
const size = (bytes) => bytes ? `${(bytes / 1048576).toFixed(1)} MB` : "";

// --- avatars -------------------------------------------------------------------

const FACE_SVG = {
  smile: `<ellipse cx="-9" cy="-4" rx="3.5" ry="6" fill="#232832"/><ellipse cx="9" cy="-4" rx="3.5" ry="6" fill="#232832"/>
          <path d="M -9 8 Q 0 15 9 8" stroke="#232832" stroke-width="3" fill="none" stroke-linecap="round"/>`,
  happy: `<path d="M -14 -2 L -9 -8 L -4 -2 M 4 -2 L 9 -8 L 14 -2" stroke="#232832" stroke-width="3" fill="none" stroke-linecap="round"/>
          <path d="M -10 6 Q 0 17 10 6 Z" fill="#232832"/>`,
  surprised: `<circle cx="-9" cy="-4" r="5" fill="none" stroke="#232832" stroke-width="2.6"/>
              <circle cx="9" cy="-4" r="5" fill="none" stroke="#232832" stroke-width="2.6"/><ellipse cx="0" cy="10" rx="3.5" ry="5" fill="#232832"/>`,
  determined: `<ellipse cx="-9" cy="-2" rx="3.5" ry="5" fill="#232832"/><ellipse cx="9" cy="-2" rx="3.5" ry="5" fill="#232832"/>
               <path d="M -15 -12 L -4 -8 M 15 -12 L 4 -8 M -8 11 L 8 11" stroke="#232832" stroke-width="3" fill="none" stroke-linecap="round"/>`,
};

// --- 3D avatars ----------------------------------------------------------------
// Drawn with WebGL from /avatar-model.json, which the game's renderer writes
// (brixo-render's web_model test), so the site shows exactly what you look
// like in a game: same body, same faces, same hats.

let MODEL = null;
const modelReady = fetch("/avatar-model.json").then((r) => r.json()).then((m) => {
  const bytes = (b64) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)).buffer;
  const mesh = (m) => ({
    p: Float32Array.from(new Int16Array(bytes(m.p)), (v) => v / 1000),
    n: Float32Array.from(new Int8Array(bytes(m.n)), (v) => v / 127),
    uv: m.uv ? Float32Array.from(new Uint16Array(bytes(m.uv)), (v) => v / 65535) : null,
  });
  MODEL = {
    cell: m.cell,
    maxHats: m.max_hats,
    body: m.body.map((b) => ({ slot: b.slot, limb: b.limb, ...mesh(b) })),
    hats: m.hats.map((h) => ({ name: h.name, title: h.title, pieces: h.pieces.map((p) => ({ color: p.color, ...mesh(p) })) })),
    faces: Object.fromEntries(Object.entries(m.faces).map(([k, v]) => [k, new Uint8Array(bytes(v))])),
  };
  return MODEL;
});

const AV_VS = `attribute vec3 p; attribute vec3 n; attribute vec2 uv;
uniform mat4 mvp; uniform mat4 turn; varying vec3 vn; varying vec2 vuv;
void main() { gl_Position = mvp * vec4(p, 1.0); vn = (turn * vec4(n, 0.0)).xyz; vuv = uv; }`;
const AV_FS = `precision mediump float;
uniform vec3 color; uniform sampler2D face; uniform float textured; varying vec3 vn; varying vec2 vuv;
void main() {
  vec3 nn = normalize(vn);
  float light = 0.52 + 0.5 * max(dot(nn, normalize(vec3(0.45, 0.8, 0.6))), 0.0) + 0.1 * nn.y;
  if (textured > 0.5) {
    vec4 t = texture2D(face, vuv);
    if (t.a < 0.5) discard;
    gl_FragColor = vec4(t.rgb * light, 1.0);
  } else {
    gl_FragColor = vec4(color * light, 1.0);
  }
}`;

// One WebGL drawing surface: a canvas, its buffers and face textures.
class AvatarGL {
  constructor(canvas) {
    this.canvas = canvas;
    const gl = this.gl = canvas.getContext("webgl", { antialias: true, preserveDrawingBuffer: true, alpha: true });
    if (!gl) return;
    const shader = (type, src) => { const s = gl.createShader(type); gl.shaderSource(s, src); gl.compileShader(s); return s; };
    const prog = this.prog = gl.createProgram();
    gl.attachShader(prog, shader(gl.VERTEX_SHADER, AV_VS));
    gl.attachShader(prog, shader(gl.FRAGMENT_SHADER, AV_FS));
    gl.linkProgram(prog);
    this.loc = Object.fromEntries(["p", "n", "uv"].map((k) => [k, gl.getAttribLocation(prog, k)]));
    this.u = Object.fromEntries(["mvp", "turn", "color", "face", "textured"].map((k) => [k, gl.getUniformLocation(prog, k)]));
    this.buffers = new Map();
    this.textures = {};
  }
  buffer(mesh) {
    const gl = this.gl;
    let b = this.buffers.get(mesh);
    if (!b) {
      const make = (data) => { if (!data) return null; const x = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, x); gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW); return x; };
      b = { p: make(mesh.p), n: make(mesh.n), uv: make(mesh.uv), count: mesh.p.length / 3 };
      this.buffers.set(mesh, b);
    }
    return b;
  }
  faceTexture(name) {
    const gl = this.gl;
    if (!this.textures[name]) {
      const cell = MODEL.cell, bits = MODEL.faces[name] || MODEL.faces.smile;
      const px = new Uint8Array(cell * cell * 4);
      for (let i = 0; i < cell * cell; i++) {
        if (bits[i >> 3] & (1 << (i & 7))) px.set([20, 20, 20, 255], i * 4);
      }
      const t = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, cell, cell, 0, gl.RGBA, gl.UNSIGNED_BYTE, px);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      this.textures[name] = t;
    }
    return this.textures[name];
  }
  // look: {skin, shirt, pants, face, hats}; view: {yaw, headOnly, onlyHat}
  draw(look, view = {}) {
    const gl = this.gl;
    if (!gl || !MODEL) return;
    const w = this.canvas.width, h = this.canvas.height;
    gl.viewport(0, 0, w, h);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.DEPTH_TEST);
    gl.enable(gl.CULL_FACE);
    gl.useProgram(this.prog);
    // Camera: framed on the whole character, or just the head.
    const [cy, span] = view.headOnly ? [2.45, 3.0] : [0.55, 6.9];
    const fov = 0.5, dist = span / 2 / Math.tan(fov / 2);
    const f = 1 / Math.tan(fov / 2), aspect = w / h, near = 0.5, far = 60;
    const proj = [f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) / (near - far), -1, 0, 0, 2 * far * near / (near - far), 0];
    const yaw = view.yaw ?? 0.35, pitch = view.headOnly ? 0.2 : 0.1;
    const cyw = Math.cos(yaw), syw = Math.sin(yaw), cp = Math.cos(pitch), sp = Math.sin(pitch);
    // turn = pitch * yaw (rotation only, column-major)
    const turn = [cyw, sp * syw, -cp * syw, 0, 0, cp, sp, 0, syw, -sp * cyw, cp * cyw, 0, 0, 0, 0, 1];
    const mul = (a, b) => { const o = new Array(16).fill(0); for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) for (let k = 0; k < 4; k++) o[c * 4 + r] += a[k * 4 + r] * b[c * 4 + k]; return o; };
    // The model turns around its middle at the framing height.
    const center = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, -cy, 0, 1];
    const full = mul(proj, mul([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, -dist, 1], mul(turn, center)));
    gl.uniformMatrix4fv(this.u.mvp, false, new Float32Array(full));
    gl.uniformMatrix4fv(this.u.turn, false, new Float32Array(turn));
    const color = (c) => gl.uniform3f(this.u.color, c[0] / 255, c[1] / 255, c[2] / 255);
    const bind = (loc, buf, size) => {
      if (loc < 0) return;
      if (!buf) { gl.disableVertexAttribArray(loc); gl.vertexAttrib2f(loc, 0, 0); return; }
      gl.bindBuffer(gl.ARRAY_BUFFER, buf); gl.enableVertexAttribArray(loc); gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 0, 0);
    };
    const drawMesh = (mesh) => { const b = this.buffer(mesh); bind(this.loc.p, b.p, 3); bind(this.loc.n, b.n, 3); bind(this.loc.uv, b.uv, 2); gl.drawArrays(gl.TRIANGLES, 0, b.count); };
    const colors = { skin: look.skin, shirt: look.shirt, pants: look.pants };
    for (const part of MODEL.body) {
      if (view.headOnly && part.limb !== "head") continue;
      if (part.slot === "decal") continue;
      gl.uniform1f(this.u.textured, 0);
      color(colors[part.slot] || [200, 200, 200]);
      drawMesh(part);
    }
    // The face last, printed on the head.
    gl.uniform1f(this.u.textured, 1);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.faceTexture(look.face));
    gl.uniform1i(this.u.face, 0);
    for (const part of MODEL.body) if (part.slot === "decal") drawMesh(part);
    gl.uniform1f(this.u.textured, 0);
    const worn = view.onlyHat ? [view.onlyHat] : (look.hats || []);
    for (const hat of MODEL.hats) {
      if (!worn.includes(hat.name)) continue;
      for (const piece of hat.pieces) { color(piece.color); drawMesh(piece); }
    }
  }
}

// Pictures of avatars (for the banner, profiles, the home page): drawn on
// one hidden canvas and copied into <img> tags.
let picGL = null;
function avatarPicture(look, view = {}, w = 180, h = 260) {
  if (!MODEL) return "";
  if (!picGL) picGL = new AvatarGL(document.createElement("canvas"));
  if (!picGL.gl) return "";
  const scale = 2;
  picGL.canvas.width = w * scale; picGL.canvas.height = h * scale;
  picGL.draw(look, view);
  return picGL.canvas.toDataURL();
}
function paintAvatars() {
  modelReady.then(() => {
    for (const img of document.querySelectorAll("img[data-look]")) {
      const look = JSON.parse(img.dataset.look);
      delete img.dataset.look;
      img.src = avatarPicture(look, {}, img.width || 180, img.height || 260);
    }
  });
}
// Kept under its old name: every page asks for avatarSvg(look).
function avatarSvg(a, small) {
  setTimeout(paintAvatars);
  const [w, h] = small ? [44, 54] : [180, 260];
  const look = JSON.stringify({ skin: a.skin, shirt: a.shirt, pants: a.pants, face: a.face, hats: a.hats || [] });
  return `<img class="avatar-pic" width="${w}" height="${h}" alt="" data-look='${look.replace(/'/g, "&#39;")}'>`;
}


// --- Learn pages ---------------------------------------------------------------
// Colours Rovik code the way the Studio script editor does, adds a Copy
// button to each block, and runs the sidebar's search.

const ROVIK_KEYWORDS = new Set("fn end if then elseif else while do for in return break continue and or not true false nil on every seconds".split(" "));
const ROVIK_BUILTINS = new Set(("print len str num type push pop insert remove keys wait floor round abs min max sqrt sin cos asin acos atan2 random " +
  "find destroy clone time players create play_sound play_music stop_music explode").split(" "));

function highlightRovik(src) {
  const out = [];
  const re = /(\*\*\*[\s\S]*?\*\*\*|--[^\n]*)|("(?:[^"\\\n]|\\.)*"?)|(\b\d+(?:\.\d+)?\b)|(\b[A-Za-z_][A-Za-z0-9_]*\b)|([\s\S])/g;
  let m;
  while ((m = re.exec(src))) {
    const t = esc(m[0]);
    if (m[1]) out.push(`<span class="c">${t}</span>`);
    else if (m[2]) out.push(`<span class="s">${t}</span>`);
    else if (m[3]) out.push(`<span class="n">${t}</span>`);
    else if (m[4] && ROVIK_KEYWORDS.has(m[4])) out.push(`<span class="k">${t}</span>`);
    else if (m[4] && ROVIK_BUILTINS.has(m[4]) && src[re.lastIndex] === "(") out.push(`<span class="f">${t}</span>`);
    else if (m[4] === "self") out.push(`<span class="self">${t}</span>`);
    else out.push(t);
  }
  return out.join("");
}

function learnPage() {
  for (const pre of document.querySelectorAll("pre.code")) {
    const code = pre.querySelector("code");
    const text = code.textContent;
    if (pre.classList.contains("rovik")) code.innerHTML = highlightRovik(text);
    const b = document.createElement("button");
    b.className = "copy";
    b.textContent = "Copy";
    b.onclick = async () => {
      try { await navigator.clipboard.writeText(text); b.textContent = "Copied!"; }
      catch (e) { b.textContent = "Select and copy"; }
      setTimeout(() => (b.textContent = "Copy"), 1500);
    };
    pre.parentElement.appendChild(b);
  }
  const box = document.getElementById("learn-search");
  const results = document.getElementById("learn-results");
  const index = document.getElementById("learn-index");
  let data = null;
  box.addEventListener("input", async () => {
    const q = box.value.trim().toLowerCase();
    if (!q) { results.innerHTML = ""; index.style.display = ""; return; }
    if (!data) data = await fetch("/learn/search.json").then((r) => r.json()).catch(() => []);
    const words = q.split(/\s+/);
    const scored = data.map((p) => {
      const title = p.title.toLowerCase(), heads = p.heads.toLowerCase(), text = p.text.toLowerCase();
      let score = 0;
      for (const w of words) {
        if (!text.includes(w) && !title.includes(w) && !heads.includes(w)) return null;
        score += (title.includes(w) ? 10 : 0) + (heads.includes(w) ? 4 : 0) + Math.min(text.split(w).length - 1, 5);
      }
      const at = text.indexOf(words[0]);
      const snip = p.text.slice(Math.max(0, at - 40), at + 80);
      return { p, score, snip };
    }).filter(Boolean).sort((a, b) => b.score - a.score).slice(0, 8);
    index.style.display = "none";
    results.innerHTML = scored.length
      ? scored.map((r) => `<a class="hit" href="/learn/${r.p.slug}"><b>${esc(r.p.title)}</b><span>&hellip;${esc(r.snip)}&hellip;</span></a>`).join("")
      : `<p class="hint">Nothing found for "${esc(box.value)}".</p>`;
  });
}
