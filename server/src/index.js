import express from "express";
import cors from "cors";

const app = express();
app.use(express.json());

// Autorise ton front GitHub Pages
app.use(cors({
  origin: [
    "https://pierrelecouillard-ui.github.io"
  ]
}));

app.get("/", (req, res) => {
  res.type("text").send("FFE Pre-Engage API is running");
});

app.get("/health", (req, res) => {
  res.json({ ok: true });
});


// Endpoint qui remplace l'ancien invoke("load_epreuves", { url })
app.post("/api/load-epreuves", async (req, res) => {
  const { url } = req.body || {};
  if (!url) return res.status(400).json({ error: "Missing url" });

  // TODO: ici tu mets la logique qui était dans Rust:
  // - fetch du lien concours
  // - parsing (HTML/JSON)
  // - renvoyer une liste d'épreuves
  //
  // Pour l’instant : réponse dummy
  return res.json({ epreuves: [], source: url });
});

const port = process.env.PORT || 3000;
app.listen(port, () => console.log("API listening on", port));
