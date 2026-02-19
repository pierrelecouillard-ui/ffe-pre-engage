import express from "express";
import cors from "cors";

const app = express();
app.use(express.json());

// ⚠️ ajuste si tu utilises un autre domaine / preview
app.use(
  cors({
    origin: [
      "https://pierrelecouillard-ui.github.io",
      "http://localhost:5173",
      "http://127.0.0.1:5173",
    ],
  })
);

app.get("/", (req, res) => {
  res.type("text").send("FFE Pre-Engage API is running");
});

app.get("/health", (req, res) => res.json({ ok: true }));

// -----------------------------
// Stockage simple en mémoire (free tier = redémarrages => perte des targets)
// -----------------------------
let targets = [];
let nextId = 1;

// Helpers
function decodeEntities(s) {
  return String(s ?? "")
    .replace(/&nbsp;/g, " ")
    .replace(/\u00A0/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .trim();
}

function parseEpreuves(html) {
  const out = [];
  const add = (u, label) => {
    if (!u) return;
    const full = u.startsWith("http") ? u : `https://ffecompet.ffe.com${u}`;
    if (!out.some((e) => e.url === full)) out.push({ label: label?.trim() || full, url: full });
  };

  // 1) <a href="...">
  for (const m of html.matchAll(/<a[^>]+href=["']([^"']+)["'][^>]*>([\s\S]*?)<\/a>/gi)) {
    const href = m[1];
    const text = m[2].replace(/<[^>]*>/g, " ").replace(/\s+/g, " ").trim();
    if (href && /epreuv/i.test(href)) add(href, text);
  }

  // 2) onclick=".../epreuve/..."
  for (const m of html.matchAll(/onclick=["'][^"']*(\/[^"']*epreuv[^"']*)["']/gi)) {
    add(m[1], "Épreuve");
  }

  // 3) data-href="...epreuve..."
  for (const m of html.matchAll(/data-href=["']([^"']*epreuv[^"']*)["']/gi)) {
    add(m[1], "Épreuve");
  }

  // 4) URLs d’épreuves présentes “en dur” dans le HTML/JS
  for (const m of html.matchAll(/(\/[^\s"'<>]*epreuv[^\s"'<>]*)/gi)) {
    add(m[1], "Épreuve");
  }

  return out;
}

// -----------------------------
// API commands (remplacent invoke Tauri)
// -----------------------------

// POST /api/list_epreuves { contestId }
app.post("/api/list_epreuves", async (req, res) => {
  try {
    const contestId = String(req.body?.contestId ?? "").trim();
    if (!contestId) return res.status(400).json({ error: "N° concours manquant" });

    const url = `https://ffecompet.ffe.com/concours/${contestId}`;

    const r = await fetch(url, {
      headers: {
        "user-agent":
          "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
        accept: "text/html,*/*",
      },
      redirect: "follow",
    });

    const body = await r.text();
    if (!r.ok) {
      return res.status(502).json({
        error: `HTTP ${r.status} sur ${url}`,
        snippet: body.slice(0, 240),
      });
    }

    const epreuves = parseEpreuves(body);
    return res.json(epreuves);
  } catch (e) {
    return res.status(500).json({ error: String(e?.message || e) });
  }
});

// POST /api/list_targets
app.post("/api/list_targets", (req, res) => {
  res.json(targets);
});

// POST /api/add_target { payload: {...} }
app.post("/api/add_target", (req, res) => {
  const payload = req.body?.payload ?? {};
  const label = String(payload.label ?? "").trim();
  const url = String(payload.url ?? "").trim();

  if (!label || !url) return res.status(400).json({ error: "label + url requis" });

  const t = {
    id: nextId++,
    label,
    url,
    cheval: payload.cheval ?? null,
    cavalier: payload.cavalier ?? null,
    interval_normal_sec: Number(payload.interval_normal_sec ?? 60),
    interval_hot_sec: Number(payload.interval_hot_sec ?? 15),
    hot_from: payload.hot_from ?? null,
    hot_to: payload.hot_to ?? null,
    last_status: "never",
    last_checked_at: null,
    last_error: null,
  };

  targets.push(t);
  res.json({ ok: true });
});

// POST /api/delete_target { id }
app.post("/api/delete_target", (req, res) => {
  const id = Number(req.body?.id);
  if (!Number.isFinite(id)) return res.status(400).json({ error: "id invalide" });

  const before = targets.length;
  targets = targets.filter((t) => t.id !== id);

  res.json({ ok: true, removed: before - targets.length });
});

// POST /api/start_watcher
app.post("/api/start_watcher", (req, res) => {
  // TODO: implémenter une vraie boucle serveur + notifications si tu veux
  res.json({ ok: true, running: false, note: "Watcher not implemented on web yet" });
});

// POST /api/stop_watcher
app.post("/api/stop_watcher", (req, res) => {
  res.json({ ok: true });
});

// POST /api/open_url { url }
app.post("/api/open_url", (req, res) => {
  // En web, c'est le front qui doit window.open(...) ; côté API on répond OK.
  res.json({ ok: true });
});

// fallback
app.post("/api/:cmd", (req, res) => {
  res.status(404).json({ error: `Unknown cmd: ${req.params.cmd}` });
});

const port = process.env.PORT || 3000;
app.listen(port, () => console.log("API listening on", port));
