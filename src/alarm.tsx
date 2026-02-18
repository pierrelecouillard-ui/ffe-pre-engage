import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import { invoke } from "@tauri-apps/api/core";

function Alarm() {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [started, setStarted] = useState(false);

  const contestId = new URLSearchParams(window.location.search).get("id") || "—";
  const contestUrl =
    contestId && contestId !== "—"
      ? `https://ffecompet.ffe.com/concours/${contestId}`
      : "https://ffecompet.ffe.com/";

  useEffect(() => {
    const a = new Audio("/alarm.wav");
    a.loop = true;
    audioRef.current = a;

    const tryStart = async () => {
      try {
        await a.play();
        setStarted(true);
      } catch {
        setStarted(false);
      }
    };

    // Essaye au chargement + quand la fenêtre reprend le focus
    tryStart();
    window.addEventListener("focus", tryStart);

    return () => {
      window.removeEventListener("focus", tryStart);
      a.pause();
    };
  }, []);

  function stopSound() {
    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current.currentTime = 0; // remet au début
    }
    setStarted(false);
  }

  async function openFfeSite() {
    // bonus : couper le son quand tu pars sur le site
    stopSound();

    try {
      // appelle la commande Rust open_url (plugin opener)
      await invoke("open_url", { url: contestUrl });
    } catch {
      // fallback navigateur
      window.open(contestUrl, "_blank");
    }
  }

  async function hideWindow() {
    stopSound();

    // Masquer via Tauri si dispo, sinon fermer la fenêtre
    const anyWin = window as any;
    if (anyWin.__TAURI__) {
      try {
        const mod = await import("@tauri-apps/api/webviewWindow");
        const w = mod.getCurrentWebviewWindow();
        await w.hide();
        return;
      } catch {
        // fallback
      }
    }
    window.close();
  }

  return (
    <div className="container">
      <div className="card">
        <div style={{ fontSize: 26, fontWeight: 800 }}>🚨 ALERTE ENGAGEMENT OUVERT</div>
        <div className="small" style={{ marginTop: 6 }}>
          Concours : <span className="mono">{contestId}</span>
        </div>

        <hr />

        {!started ? (
          <div className="small">
            L’audio auto peut être bloqué. Clique sur “Démarrer l’alarme”.
          </div>
        ) : (
          <div className="small">Alarme en cours…</div>
        )}

        <div className="row" style={{ marginTop: 12 }}>
          <button
            className="primary"
            onClick={async () => {
              try {
                await audioRef.current?.play();
                setStarted(true);
              } catch {}
            }}
          >
            Démarrer l’alarme
          </button>

          <button onClick={stopSound}>Stop son</button>

          <button onClick={openFfeSite}>Site FFE</button>

          <button onClick={hideWindow}>Masquer</button>
        </div>
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Alarm />
  </React.StrictMode>
);
