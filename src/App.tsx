import React, { useEffect, useMemo, useRef, useState } from "react";
import "./styles.css";
import { addTarget, deleteTarget, listTargets, startWatcher, stopWatcher, type Target } from "./api";
import invoke from "./lib/invoke";

const isTauriRuntime = () => typeof (window as any).__TAURI_INTERNALS__ !== "undefined";

export default function App() {
  const [targets, setTargets] = useState<Target[]>([]);
  // Anti-spam / anti-race pour éviter plusieurs fenêtres d'alerte simultanées
  const lastAlarmByContestRef = useRef<Record<string, number>>({});
  const openingAlarmRef = useRef(false);
  const [loading, setLoading] = useState(false);
  const [watching, setWatching] = useState(false);

  const [ffeServerOk, setFfeServerOk] = useState<boolean | null>(null);

  const ffeConnected = ffeServerOk === true;

  // Connexion FFE via ID/MDP (optionnel)
  const [ffeUser, setFfeUser] = useState("");
  const [ffePass, setFfePass] = useState("");
  const [ffeLoginError, setFfeLoginError] = useState<string | null>(null);
  const [ffeLoginLoading, setFfeLoginLoading] = useState(false);


  // ✅ États indépendants par bloc
  // --- Alerte ouverture concours
  const [contestIdConcours, setContestIdConcours] = useState("");
  const [concoursError, setConcoursError] = useState<string | null>(null);

  // --- Alerte place épreuve
  const [contestIdEpreuve, setContestIdEpreuve] = useState("");


  type Epreuve = { label: string; url: string };

  const [epreuves, setEpreuves] = useState<Epreuve[]>([]);
  const [selected, setSelected] = useState<Record<string, boolean>>({});
  const [loadingEpreuves, setLoadingEpreuves] = useState(false);
  const [epreuvesError, setEpreuvesError] = useState<string | null>(null);
  const [epreuvesOpen, setEpreuvesOpen] = useState(true);

  async function checkFFEConnection() {
    try {
      const ok = await invoke<boolean>("check_ffe_connected");
      setFfeServerOk(ok);
      return ok;
    } catch (e) {
      setFfeServerOk(false);
      return false;
    }
  }

  async function loginFFEWithCredentials() {
    setFfeLoginError(null);
    setFfeServerOk(null);
    setFfeLoginLoading(true);

    const username = ffeUser.trim();
    const password = ffePass;

    if (!username || !password) {
      setFfeLoginError("Renseigne identifiant + mot de passe.");
      setFfeServerOk(false);
      setFfeLoginLoading(false);
      return;
    }

    try {
      // ⚠️ Le backend ne doit pas logger le mot de passe.
      await invoke("ffe_login_with_credentials", { username, password });

      // Vérif serveur réelle
      const ok = await checkFFEConnection();
      setFfeServerOk(ok); // on aligne l'ancien flag sur le statut serveur
      if (!ok) setFfeLoginError("Login tenté, mais FFE ne te considère pas connecté.");
    } catch (e) {
      setFfeServerOk(false);
      setFfeLoginError(String(e));
    } finally {
      setFfeLoginLoading(false);
    }
  }

  async function logoutFFE() {
  setFfeLoginError(null);
  setFfeLoginLoading(true);
  try {
    await invoke("ffe_logout");          // <-- commande backend à créer
    setFfeServerOk(false);              // UI: repasse en "non connecté"
  } catch (e) {
    setFfeLoginError("Déconnexion impossible: " + String(e));
  } finally {
    setFfeLoginLoading(false);
  }
}
  // Intervalles indépendants par bloc
  const [intervalConcoursNormal, setIntervalConcoursNormal] = useState(300);
  const [intervalConcoursHot, setIntervalConcoursHot] = useState(45);

  const [intervalEpreuveNormal, setIntervalEpreuveNormal] = useState(300);
  const [intervalEpreuveHot, setIntervalEpreuveHot] = useState(45);

  async function refresh() {
    setLoading(true);
    try {
      const t = await listTargets();
      setTargets(t);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { refresh(); }, []);
useEffect(() => {
  // Statut initial (si une session a déjà été enregistrée côté Rust)
  checkFFEConnection();
}, []);

useEffect(() => {
  // Le backend émet "ffe:status" après save_ffe_session()
  // En mode Web (iPhone), il n'y a pas d'events Tauri -> on ignore.
  let unlisten: undefined | (() => void);

  if (isTauriRuntime()) {
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<{ connected?: boolean; cookieCount?: number }>("ffe:status", async () => {
      const wait = (ms: number) => new Promise((res) => setTimeout(res, ms));

      let ok = false;
      for (let i = 0; i < 30; i++) { // ~15s max
        ok = await checkFFEConnection(); // invoke("check_ffe_connected")
        if (ok) break;
        await wait(500);
      }

      setFfeServerOk(ok);
      setFfeLoginError(ok ? null : "Connexion FFE non confirmée (redirection SSO en cours ou session absente).");
      setFfeLoginLoading(false);
    });
    })().catch(() => {});
  }

  return () => {
    if (unlisten) unlisten();
  };
}, []);

useEffect(() => {
    const it = setInterval(() => { refresh(); }, 2000);
    return () => clearInterval(it);
  }, []);


  const openCount = useMemo(
    () => targets.filter(t => t.last_status === "OPEN").length,
    [targets]
  );

  function buildContestUrl(id: string): string {
    return "https://ffecompet.ffe.com/concours/" + id;
  }

  async function loadEpreuves() {
    // Mode public: on tente même sans connexion FFE (si le backend exige une session, il renverra une erreur lisible)
    if (!ffeConnected) {
      setEpreuvesError("Non connecté FFE : tentative de chargement en mode public…");
    }
    const id = contestIdEpreuve.trim();
    if (!/^[0-9]{9}$/.test(id)) { setEpreuvesError("N° concours invalide (9 chiffres)."); return; }

    setLoadingEpreuves(true);
    setEpreuvesError(null);
    try {
      const eps = await invoke<{ label: string; url: string }[]>("list_epreuves", { contestId: id });
      setEpreuves(eps);
      if (eps.length > 0) setEpreuvesOpen(true);

      const init: Record<string, boolean> = {};
      for (const e of eps) init[e.url] = false;
      setSelected(init);

      if (eps.length === 0) {
        setEpreuvesError("Aucune épreuve détectée. (Le parse HTML doit être ajusté : envoie-moi l’URL exacte du concours.)");
      }
    } catch (e) {
      console.error("list_epreuves error", e);
      setEpreuves([]);
      setSelected({});
      setEpreuvesError(String(e));
    } finally {
      setLoadingEpreuves(false);
    }
  }

  async function addSelectedEpreuves() {
    const id = contestIdEpreuve.trim();
    const picked = epreuves.filter(e => selected[e.url]);

    for (const e of picked) {
      await addTarget({
        label: `${id} — ${e.label}`,
        url: e.url,
        cheval: null,
        cavalier: null,
        interval_normal_sec: intervalEpreuveNormal,
        interval_hot_sec: intervalEpreuveHot
      });
    }
    await refresh();
  }

  // ✅ Ouvre la fenêtre Tauri d'alerte "concours" (alarm.html) avec le N° concours
async function openAlarmConcoursWindow(id: string) {
  const relative = `alarm.html?id=${encodeURIComponent(id)}`;
  const url = new URL(relative, window.location.href).toString();

  if (!isTauriRuntime()) {
    // Web/iPhone: ouvre dans un nouvel onglet (Safari)
    window.open(url, "_blank");
    return;
  }

  // Tauri desktop: fenêtre dédiée
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");

  let existing: any = null;
  try {
    existing = await WebviewWindow.getByLabel("alarm");
  } catch {}
  if (existing) {
    await existing.show().catch(() => {});
    await existing.setFocus().catch(() => {});
    return;
  }

  const win = new WebviewWindow("alarm", {
    url: relative,
    title: "ALERTE CONCOURS",
    alwaysOnTop: true,
    focus: true,
    resizable: false,
    width: 520,
    height: 280
  });

  win.once("tauri://error", () => {});
}

// ✅ Ouvre la fenêtre Tauri d'alerte "épreuve" (alarm_epreuve.html)
async function openAlarmEpreuveWindow(label: string) {
  const relative = `alarm_epreuve.html?label=${encodeURIComponent(label)}`;
  const url = new URL(relative, window.location.href).toString();

  if (!isTauriRuntime()) {
    window.open(url, "_blank");
    return;
  }

  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");

  let existing: any = null;
  try {
    existing = await WebviewWindow.getByLabel("alarm_epreuve");
  } catch {}
  if (existing) {
    await existing.show().catch(() => {});
    await existing.setFocus().catch(() => {});
    return;
  }

  const win = new WebviewWindow("alarm_epreuve", {
    url: relative,
    title: "PLACE LIBRE",
    alwaysOnTop: true,
    focus: true,
    resizable: false,
    width: 560,
    height: 320
  });

  win.once("tauri://error", () => {});
}

  // ✅ Quand Rust émet l'évènement "target_open", on ouvre la fenêtre d'alerte
useEffect(() => {
  // ✅ Quand Rust émet l'évènement "target_open", on ouvre la fenêtre d'alerte
  // En mode Web (iPhone), il n'y a pas d'events Tauri -> on ignore.
  let unlisten: undefined | (() => void);

  if (isTauriRuntime()) {
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<{ id: number; label: string; url: string }>("target_open", (e) => {
    const contest = (e.payload?.label || "").trim();
    if (!contest) return;
    const targetUrl = (e.payload?.url || "").trim();
    const isEpreuve = targetUrl.includes("watch_epreuve=") || /epreuve/i.test(contest);

    // Déclenchement unique (anti-spam):
    // 1) on ignore les doublons du même concours sur une courte fenêtre
    // 2) on évite les courses (plusieurs events quasi simultanés) qui créent plusieurs fenêtres
    const now = Date.now();
    const last = lastAlarmByContestRef.current[contest] ?? 0;
    if (now - last < 30_000) return; // 30s de cooldown par concours
    if (openingAlarmRef.current) return;

    lastAlarmByContestRef.current[contest] = now;
    openingAlarmRef.current = true;
    void (isEpreuve ? openAlarmEpreuveWindow(contest) : openAlarmConcoursWindow(contest)).finally(() => {
      openingAlarmRef.current = false;
    });
  });
    })().catch(() => {});
  }

  return () => {
    if (unlisten) unlisten();
  };
}, []);

  async function onAddConcours() {
    setConcoursError(null);

    const id = contestIdConcours.trim();
    if (!/^[0-9]{9}$/.test(id)) {
      setConcoursError("N° concours invalide (9 chiffres).");
      return;
    }

    await addTarget({
      // label = n° concours
      label: id,
      // l\'URL est construite en interne (pas affichée)
      url: buildContestUrl(id),
      cheval: null,
      cavalier: null,
      interval_normal_sec: intervalConcoursNormal,
      interval_hot_sec: intervalConcoursHot
    });

    setContestIdConcours("");
    await refresh();
  }

  return (
    <div className="container">
      <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
        <div>
          <div style={{ fontSize: 22, fontWeight: 700 }}>FFE — Alerte ouverture engagements</div>
          <div className="small">Saisie: N° concours • Notification dès que l’engagement passe à OPEN</div>
        </div>

        <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 8 }}>
          {/* Statut FFE (le bouton est dans "Connexion FFE (ID/MDP)" plus bas) */}

          <div className="row" style={{ gap: 8, justifyContent: "flex-end" }}>
            {ffeServerOk === true ? (
              <span className="badge OPEN">FFE connecté ✅</span>
            ) : ffeServerOk === false ? (
              <span className="badge">FFE non connecté ❌</span>
            ) : (
              <span className="badge">FFE: …</span>
            )}

          </div>
        </div>
      </div>

      <div className="card" style={{ marginTop: 12 }}>
        <div style={{ fontWeight: 700, marginBottom: 8 }}>Connexion FFE (ID/MDP)</div>
        <div className="grid grid-2" style={{ alignItems: "end" }}>
          <div>
              <input value={ffeUser} onChange={e => setFfeUser(e.target.value)} placeholder="Identifiant FFE"  className="input-s"/>
          </div>
          <div>
            <input value={ffePass} onChange={e => setFfePass(e.target.value)} placeholder="Mot de passe FFE" type="password"  className="input-s"/>
          </div>
        </div>
        
<div className="row" style={{ marginTop: 10, justifyContent: "flex-start", gap: 8 }}>
  <button
    className="primary"
    onClick={loginFFEWithCredentials}
    disabled={ffeLoginLoading || ffeConnected}
    style={ffeConnected ? { backgroundColor: "#16a34a", borderColor: "#16a34a" } : undefined}
  >
    {ffeConnected ? "Connecté" : ffeLoginLoading ? "Connexion…" : "Se connecter"}
  </button>

  {ffeConnected ? (
    <button
      className="secondary"
      onClick={logoutFFE}
      disabled={ffeLoginLoading}
    >
      Déconnexion
    </button>
  ) : null}
</div>


        {ffeLoginLoading ? (
          <div className="small" style={{ marginTop: 8, opacity: 0.9 }}>Connexion en cours… merci de patienter.</div>
        ) : null}
        {ffeLoginError ? <div className="small" style={{ marginTop: 8 }}>Erreur: {ffeLoginError}</div> : null}
      </div>

      <div className="grid grid-2" style={{ marginTop: 12 }}>
        <div className="card">
          <div style={{ fontWeight: 700, marginBottom: 8 }}>ALERTE OUVERTURE CONCOURS</div>

<div className="row" style={{ gap: 8, alignItems: "center", marginBottom: 10 }}>
</div>
          <div className="grid">
            <div>
              <div className="small">N° concours (9 chiffres)</div>
              <input value={contestIdConcours} onChange={e => setContestIdConcours(e.target.value)} placeholder="202635080"  className="input-s"/>
            </div>

            <div className="grid grid-2">
              <div>
                <div className="small">Intervalle normal (sec)</div>
                <input
  type="number"
  value={intervalConcoursNormal}
  onChange={e => setIntervalConcoursNormal(Number(e.target.value || 0))}
 className="input-xs"/>
              </div>
              <div>
                <div className="small">Intervalle chaud (sec)</div>
                <input value={intervalConcoursHot} onChange={e => setIntervalConcoursHot(Number(e.target.value || 0))}  className="input-xs"/>
              </div>
            </div>

            <button className="row" style={{ gap: 10, flexWrap: "wrap" }} onClick={onAddConcours}>Charger concours</button>
            {concoursError ? <div className="small" style={{ marginTop: 8 }}>Erreur: {concoursError}</div> : null}

              {!watching ? (
    <button
      className="primary"
      onClick={async () => {
        await startWatcher();
        setWatching(true);
      }}
    >
      Créer alerte d'ouverture
    </button>
  ) : (
    <button
      onClick={async () => {
        await stopWatcher();
        setWatching(false);
      }}
    >
      Stop
    </button>
  )}
  {watching ? <span className="badge OPEN">En cours</span> : <span className="badge">Arrêté</span>}
          </div>
        </div>

        <div className="card">
          <div style={{ fontWeight: 700, marginBottom: 8 }}>ALERTE PLACE EPREUVE</div>

          <div className="grid" style={{ marginBottom: 10 }}>
            <div>
              <div className="small">N° concours (9 chiffres)</div>
              <input value={contestIdEpreuve} onChange={e => setContestIdEpreuve(e.target.value)} placeholder="202635080"  className="input-s"/>
            </div>

            <div className="grid grid-2">
              <div>
                <div className="small">Intervalle normal (sec)</div>
                <input type="number" value={intervalEpreuveNormal} onChange={e => setIntervalEpreuveNormal(Number(e.target.value || 0))}  className="input-xs"/>
              </div>
              <div>
                <div className="small">Intervalle chaud (sec)</div>
                <input type="number" value={intervalEpreuveHot} onChange={e => setIntervalEpreuveHot(Number(e.target.value || 0))}  className="input-xs"/>
              </div>
            </div>
          </div>

          <div className="row" style={{ gap: 10, flexWrap: "wrap" }}>
            <button
              onClick={loadEpreuves}
              disabled={loadingEpreuves || !/^[0-9]{9}$/.test(contestIdEpreuve.trim())}
            >
              {loadingEpreuves ? "Chargement…" : "Charger épreuves"}
            </button>
            {epreuves.length > 0 ? (
              <button className="primary" onClick={addSelectedEpreuves}>
                Créer alertes (épreuves cochées)
              </button>
            ) : null}
          </div>

          {epreuvesError ? <div className="small" style={{ marginTop: 8 }}>Erreur: {epreuvesError}</div> : null}

          {epreuves.length > 0 ? (
            <div className="card" style={{ padding: 10, marginTop: 10 }}>
              <div className="row" style={{ justifyContent: "space-between", alignItems: "center", marginBottom: 8 }}>
                <div style={{ fontWeight: 700 }}>Épreuves détectées</div>
                <button
                  className="secondary"
                  onClick={() => setEpreuvesOpen(v => !v)}
                  style={{ padding: "6px 10px" }}
                >
                  {epreuvesOpen ? "Masquer" : "Afficher"}
                </button>
              </div>
              <div className="small" style={{ marginBottom: 8, opacity: 0.8 }}>
                Coche les épreuves à surveiller (alerte quand une place se libère, ex: 60/60 → 59/60).
              </div>
              {epreuvesOpen ? (
                <div className="grid" style={{ gap: 8 }}>
                  {epreuves.map(e => {
                    const checked = !!selected[e.url];
                    return (
                      <div
                        key={e.url}
                        // IMPORTANT: on évite <label> car styles.css peut forcer `label { display: contents !important; }`
                        // ce qui "sépare" visuellement la checkbox et le texte dans la grille.
                        onClick={() => setSelected(s => ({ ...s, [e.url]: !checked }))}
                        style={{
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "flex-start",
                          gap: 10,
                          width: "100%",
                          margin: 0,
                          cursor: "pointer",
                          userSelect: "none"
                        }}
                      >
                        <input
                          type="checkbox"
                          checked={checked}
                          onClick={(ev) => ev.stopPropagation()}
                          onChange={(ev) => setSelected(s => ({ ...s, [e.url]: ev.target.checked }))}
                          style={{ margin: 0, flex: "0 0 auto" }}
                        />
                        <span style={{ fontWeight: 600, lineHeight: 1.2, flex: "1 1 auto" }}>
                          {e.label}
                        </span>
                      </div>
                    );
                  })}
                </div>
              ) : null}
            </div>
          ) : null}

          <div className="small" style={{ marginTop: 10, opacity: 0.7 }}>
            Astuce: intervalle normal 300–600s. Quand tu approches de l'ouverture, descends à 30–60s.
          </div>
        </div>
      </div>

      <div className="card" style={{ marginTop: 12 }}>
        <div className="row" style={{ justifyContent: "space-between" }}>
          <div style={{ fontWeight: 700 }}>Alertes</div>
          <button onClick={refresh}>Rafraîchir</button>
        </div>
        <hr />
        <div className="grid">
          {targets.map(t => (
            <div key={t.id} className="card" style={{ padding: 12 }}>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontWeight: 700 }}>Concours {t.label}</div>
                  <div className="small">
                    Dernière vérif: {t.last_checked_at ? new Date((t.last_checked_at as number) * 1000).toLocaleTimeString() : "—"}
                  </div>
                  {t.last_error ? <div className="small">Erreur: {t.last_error}</div> : null}
                </div>
                <div className="row">
                  <span className={`badge ${t.last_status}`}>{t.last_status}</span>
                  <button onClick={async () => {
                  try {
                    // évite les verrous SQLite pendant que le watcher écrit
                    await stopWatcher();
                    setWatching(false);

                    await deleteTarget(t.id);
                    await refresh();
                  } catch (e) {
                    alert(`Suppression impossible: ${String(e)}`);
                  }
                }}>Suppr</button>
                </div>
              </div>
            </div>
          ))}
          {targets.length === 0 ? <div className="small">Aucune alerte. Ajoute un N° concours.</div> : null}
        </div>
      </div>
    </div>
  );
}