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
              ["profile", null, "Profile"], ["download", "/download", "Get Brixo"]];

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
    if (key === "avatar" && !me) return "";
    return `<a class="tab ${page === key ? "on" : ""}" href="${href}">${label}</a>`;
  }).join("");
  document.getElementById("nav").innerHTML = `<div class="tabs">${tabs}</div><div class="online" id="online"></div>`;
  document.getElementById("footer").innerHTML =
    `<div><a href="/">Home</a>|<a href="/games">Games</a>|<a href="/download">Get Brixo</a>${me ? "" : `|<a href="/signup">Sign up</a>`}</div>
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

// The Brixo avatar: blocky body, round head, face decal. Same palettes as
// the game, so what you pick here is exactly what you'll look like.
// `small` leaves out the shadow (for the banner).
function avatarSvg(a, small) {
  const shade = (c, f) => rgb(c.map((v) => Math.round(v * f)));
  return `<svg viewBox="0 0 220 320" xmlns="http://www.w3.org/2000/svg" preserveAspectRatio="xMidYMid meet">
    ${small ? "" : `<ellipse cx="110" cy="304" rx="58" ry="10" fill="black" opacity=".2"/>`}
    <rect x="58" y="228" width="44" height="18" rx="4" fill="${rgb(a.shoes)}"/>
    <rect x="118" y="228" width="44" height="18" rx="4" fill="${rgb(a.shoes)}"/>
    <rect x="60" y="160" width="42" height="70" fill="${rgb(a.pants)}"/>
    <rect x="118" y="160" width="42" height="70" fill="${shade(a.pants, .88)}"/>
    <rect x="58" y="74" width="104" height="90" fill="${rgb(a.shirt)}"/>
    <rect x="58" y="74" width="104" height="8" fill="${shade(a.shirt, 1.12)}"/>
    <rect x="24" y="76" width="34" height="66" fill="${shade(a.shirt, .9)}"/>
    <rect x="162" y="76" width="34" height="66" fill="${shade(a.shirt, .9)}"/>
    <rect x="26" y="142" width="30" height="24" fill="${rgb(a.skin)}"/>
    <rect x="164" y="142" width="30" height="24" fill="${rgb(a.skin)}"/>
    <circle cx="110" cy="40" r="38" fill="${rgb(a.skin)}"/>
    <g transform="translate(110 44) scale(1.2)">${FACE_SVG[a.face] || FACE_SVG.smile}</g>
  </svg>`;
}
