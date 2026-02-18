#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod detect;
mod models;
mod watcher;
mod epreuves;

use std::sync::{Arc, Mutex};
use rusqlite::Connection;
use models::{AddTargetPayload, Target};
use watcher::Watcher;
use tauri::{Manager, State, Emitter, WebviewUrl, WebviewWindowBuilder, Position, LogicalPosition};
use tauri_plugin_opener::OpenerExt;
use std::sync::OnceLock;

// Cookie jar partagé (session FFE)
static FFE_COOKIES: OnceLock<Mutex<String>> = OnceLock::new();


// Debug du dernier essai de connexion (pour affichage côté UI)
static LAST_LOGIN_DEBUG: OnceLock<Mutex<String>> = OnceLock::new();
// Dernier "probe" (href/title) renvoyé par une WebView
static PROBE_STATE: OnceLock<Mutex<(String, String)>> = OnceLock::new();

fn set_login_debug_inner(msg: impl Into<String>) {
  let cell = LAST_LOGIN_DEBUG.get_or_init(|| Mutex::new(String::new()));
  *cell.lock().unwrap() = msg.into();
}

#[tauri::command]
fn set_login_debug(msg: String) -> Result<(), String> {
  set_login_debug_inner(msg);
  Ok(())
}

#[tauri::command]
fn get_login_debug() -> String {
  LAST_LOGIN_DEBUG
    .get()
    .map(|m| m.lock().unwrap().clone())
    .unwrap_or_else(|| "no debug".to_string())
}

#[tauri::command]
fn set_probe(href: String, title: String) -> Result<(), String> {
  let cell = PROBE_STATE.get_or_init(|| Mutex::new((String::new(), String::new())));
  *cell.lock().unwrap() = (href, title);
  Ok(())
}

fn take_probe() -> Option<(String, String)> {
  PROBE_STATE.get().map(|m| {
    let mut g = m.lock().unwrap();
    let out = (g.0.clone(), g.1.clone());
    // ✅ évite de relire une ancienne valeur (race condition)
    g.0.clear();
    g.1.clear();
    out
  })
}


struct AppState {
  watcher: Arc<Watcher>,
  handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
  db_path: Mutex<Option<std::path::PathBuf>>,
}

fn ensure_db(app: &tauri::AppHandle) -> anyhow::Result<std::path::PathBuf> {
  let app_data = app.path().app_data_dir()?;
  std::fs::create_dir_all(&app_data)?;
  let path = db::db_path(&app_data);
  let conn = Connection::open(&path)?;
  db::init_db(&conn)?;
  Ok(path)
}

fn get_db_path(app: &tauri::AppHandle, state: &State<AppState>) -> Result<std::path::PathBuf,String>{
  let mut guard = state.db_path.lock().unwrap();
  if guard.is_none() {
    *guard = Some(ensure_db(app).map_err(|e| e.to_string())?);
  }
  Ok(guard.clone().unwrap())
}

#[tauri::command]
fn list_targets(app: tauri::AppHandle, state: State<AppState>) -> Result<Vec<Target>, String> {
  let db_path = get_db_path(&app,&state)?;
  let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
  db::list_targets(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn add_target(app: tauri::AppHandle, state: State<AppState>, payload: AddTargetPayload) -> Result<(), String> {
  let db_path = get_db_path(&app,&state)?;
  let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
  db::add_target(&conn, payload).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_target(app: tauri::AppHandle, state: State<AppState>, id: i64) -> Result<(), String> {
  let db_path = get_db_path(&app,&state)?;
  let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
  db::delete_target(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_watcher(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
  if state.watcher.is_running() {
    return Ok(());
  }

  let db_path = get_db_path(&app,&state)?;

  // callback notification propre
  let app_handle = app.clone();
let notify_cb: Arc<dyn Fn(i64, String, String) + Send + Sync> = Arc::new(move |target_id, label, url| {
  let _ = app_handle.emit(
    "target_open",
    serde_json::json!({ "id": target_id, "label": label, "url": url })
  );
});

  let watcher = state.watcher.clone();
  let mut handle_guard = state.handle.lock().unwrap();
  let join = tokio::spawn(async move {
    watcher.run_loop(db_path, notify_cb).await;
  });
  *handle_guard = Some(join);
  Ok(())
}

#[tauri::command]
async fn stop_watcher(state: State<'_, AppState>) -> Result<(), String> {
  state.watcher.stop();
  let mut handle_guard = state.handle.lock().unwrap();
  if let Some(h) = handle_guard.take() {
    h.abort();
  }
  Ok(())
}

#[tauri::command]
fn open_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
  app.opener()
    .open_url(url, None::<&str>)
    .map_err(|e| e.to_string())
}


#[tauri::command]
fn save_ffe_session(app: tauri::AppHandle) -> Result<usize, String> {
  // Récupère les cookies du webview "ffe-login" et les stocke en mémoire
  let window = app
    .get_webview_window("ffe-login")
    .ok_or("fenêtre login introuvable (label: ffe-login)")?;

  let cookies = window.cookies().map_err(|e| e.to_string())?;
  let count = cookies.len();

  // On construit un header Cookie simple (sans domaine/path), suffisant pour réutiliser la session
  let cookie_header = cookies
    .into_iter()
    .map(|c| c.to_string())
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    // On ne garde que "name=value" (on coupe avant les attributs éventuels)
    .map(|s| s.split(';').next().unwrap_or("").trim().to_string())
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join("; ");

  let cell = FFE_COOKIES.get_or_init(|| Mutex::new(String::new()));
  *cell.lock().unwrap() = cookie_header;

  // Notifie l'UI que des cookies ont été enregistrés (l'UI doit revalider la connexion via check_ffe_connected)
  let _ = app.emit(
    "ffe:status",
    serde_json::json!({ "cookieCount": count }),
  );

  Ok(count)
}

#[tauri::command]
fn is_ffe_connected() -> bool {
  FFE_COOKIES
    .get()
    .map(|m| !m.lock().unwrap().trim().is_empty())
    .unwrap_or(false)
}



#[tauri::command]
async fn check_ffe_connected(app: tauri::AppHandle) -> Result<bool, String> {
  // 1) Si la fenêtre de login existe, on probe directement dedans (cookies garantis côté navigateur)
  if let Some(w) = app.get_webview_window("ffe-login") {
    // Après une connexion "réussie", il peut y avoir des redirections SSO pendant quelques secondes.
    // On "probe" plusieurs fois avant de conclure à un échec.
    for _ in 0..30 {
      let js = r#"
        try {
          const txt = (document.body?.innerText || "").toLowerCase();
          const hasLogout =
            txt.includes("déconnexion") ||
            !!document.querySelector("a[href*='logout'], a[href*='deconnexion'], button[aria-label*='déconnexion' i]");
          window.__TAURI__.core.invoke('set_probe', {
            href: window.location.href,
            title: document.title + " | hasLogout=" + (hasLogout ? "true" : "false")
          });
        } catch (e) {}
      "#;
      let _ = w.eval(js);

      tokio::time::sleep(std::time::Duration::from_millis(500)).await;

      if let Some((href, title)) = take_probe() {
        if href.trim().is_empty() {
          continue;
        }

        let has_logout = title.contains("hasLogout=true");
        let href_lc = href.to_lowercase();
        let is_login = href_lc.contains("sso.ffe.com/login")
          || href_lc.contains("/login")
          || href_lc.contains("/identification");

        set_login_debug_inner(format!("probe(retry): href={} | title={}", href, title));

        // ✅ priorité au marqueur DOM "Déconnexion" (session OK), l'URL peut encore être en redirection
        if has_logout {
          return Ok(true);
        }

        // fallback URL
        if !is_login {
          return Ok(true);
        }
      }
    }

    return Ok(false);
  }

  // 2) Sinon, on ouvre une WebView invisible de test sur ffecompet (même store navigateur)
  let url = tauri::Url::parse("https://ffecompet.ffe.com/").map_err(|e| e.to_string())?;
  let w = WebviewWindowBuilder::new(&app, "ffe-probe", WebviewUrl::External(url))
    .title("ffe-probe")
    .visible(false)
    .skip_taskbar(true)
    .resizable(false)
    .decorations(false)
    .build()
    .map_err(|e| e.to_string())?;

  tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

  let js = r#"
    try {
      const txt = (document.body?.innerText || "").toLowerCase();
      const hasLogout =
        txt.includes("déconnexion") ||
        !!document.querySelector("a[href*='logout'], a[href*='deconnexion'], button[aria-label*='déconnexion' i]");
      window.__TAURI__.core.invoke('set_probe', {
        href: window.location.href,
        title: document.title + " | hasLogout=" + (hasLogout ? "true" : "false")
      });
    } catch (e) {}
  "#;
  let _ = w.eval(js);
  tokio::time::sleep(std::time::Duration::from_millis(250)).await;

  let (href, title) = take_probe().unwrap_or_else(|| ("".into(), "".into()));
  let _ = w.close();

  if href.trim().is_empty() {
    set_login_debug_inner("probe(ffe-probe): no href returned".to_string());
    return Ok(false);
  }

  let has_logout = title.contains("hasLogout=true");
  let href_lc = href.to_lowercase();

  let connected = if has_logout {
    true
  } else {
    !(href_lc.contains("sso.ffe.com/login")
      || href_lc.contains("/login")
      || href_lc.contains("/identification"))
  };

  if !connected {
    set_login_debug_inner(format!("probe(ffe-probe): href={} | title={}", href, title));
  }

  Ok(connected)
}



#[tauri::command]
async fn ffe_login_with_credentials(app: tauri::AppHandle, username: String, password: String) -> Result<(), String> {
  let username = username.trim().to_string();
  if username.is_empty() || password.is_empty() {
    return Err("Identifiant et mot de passe requis.".into());
  }

  // Ouvre (ou réutilise) une WebView FFE, car le login FFE dépend du navigateur (JS/SSO/protections).
  // Objectif: garder une UI "connexion par ID", mais exécuter la connexion dans un vrai moteur WebView.
  let login_url = "https://ffecompet.ffe.com/login";
  let parsed = tauri::Url::parse(login_url).map_err(|e| e.to_string())?;

  let window = if let Some(w) = app.get_webview_window("ffe-login") {
  // Réutilise la fenêtre existante, mais on s'assure qu'elle reste invisible et hors écran.
  let _ = w.hide();
  let _ = w.set_position(Position::Logical(LogicalPosition { x: -10_000.0, y: -10_000.0 }));
  w
} else {
  WebviewWindowBuilder::new(&app, "ffe-login", WebviewUrl::External(parsed))
    .title("Connexion FFE")              // optionnel
    .visible(false)                      // 👈 invisible
    .skip_taskbar(true)                  // 👈 pas dans la barre des tâches (Windows)
    .resizable(false)
    .decorations(true)
    .transparent(true)                   // optionnel
    .inner_size(980.0, 760.0)            // taille interne
    .position(-10_000.0, -10_000.0) // 👈 hors écran (sécurité)
    .build()
    .map_err(|e| e.to_string())?
};


  // Injecte un script "best effort" qui:
  // - tente d'accepter le bandeau cookies
  // - trouve un éventuel iframe contenant le formulaire
  // - remplit identifiant + mot de passe
  // - clique "Se connecter" / "Connexion" ou submit
  //
  // NB: on n'essaie PAS de contourner des challenges; si captcha/2FA, l'utilisateur terminera à la main.
  let js = format!(r#"
    (function() {{
      const USER = {user_json};
      const PASS = {pass_json};

      function qAll(root, sel) {{ try {{ return Array.from(root.querySelectorAll(sel)); }} catch(e) {{ return []; }} }}
      function byText(root, tag, re) {{
        return qAll(root, tag).find(el => re.test((el.innerText || el.value || el.getAttribute('aria-label') || '').trim()));
      }}

      function tryAcceptCookies(doc) {{
        const patterns = [/tout accepter/i, /accepter/i, /^ok$/i, /d'accord/i, /fermer/i];
        const btns = qAll(doc, 'button, a, input[type=button], input[type=submit]');
        for (const b of btns) {{
          const t = (b.innerText || b.value || b.getAttribute('aria-label') || '').trim();
          if (patterns.some(p => p.test(t))) {{
            try {{ b.click(); return true; }} catch(e) {{}}
          }}
        }}
        // didomi fallback
        const didomi = doc.querySelector('#didomi-notice-agree-button');
        if (didomi) {{ try {{ didomi.click(); return true; }} catch(e) {{}} }}
        return false;
      }}

      function findFormDoc() {{
        // Cherche un iframe qui contient un password
        const iframes = qAll(document, 'iframe');
        for (const fr of iframes) {{
          try {{
            const d = fr.contentDocument;
            if (d && d.querySelector('input[type=password]')) return d;
          }} catch(e) {{}}
        }}
        return document;
      }}

      function fillAndSubmit(doc) {{
        tryAcceptCookies(doc);

        const userSelectors = [
          "input[name='username']",
          "input[name='email']",
          "input[name='login']",
          "input[type='email']",
          "input[placeholder*='email' i]",
          "input[placeholder*='identifiant' i]",
          "input[placeholder*='licence' i]",
        ];
        const passSelectors = [
          "input[name='password']",
          "input[type='password']",
          "input[placeholder*='mot de passe' i]",
          "input[placeholder*='password' i]",
          "input[placeholder*='code sif' i]",
        ];

        let u = null;
        for (const s of userSelectors) {{ u = doc.querySelector(s); if (u) break; }}
        let p = null;
        for (const s of passSelectors) {{ p = doc.querySelector(s); if (p) break; }}

        if (u) {{
          u.focus();
          u.value = USER;
          u.dispatchEvent(new Event('input', {{ bubbles: true }}));
          u.dispatchEvent(new Event('change', {{ bubbles: true }}));
        }}
        if (p) {{
          p.focus();
          p.value = PASS;
          p.dispatchEvent(new Event('input', {{ bubbles: true }}));
          p.dispatchEvent(new Event('change', {{ bubbles: true }}));
        }}

        const reBtn = /(se connecter|connexion|s'identifier)/i;
        let btn = byText(doc, 'button', reBtn) || byText(doc, 'input[type=submit], input[type=button]', reBtn);
        if (btn) {{
          try {{ btn.click(); return true; }} catch(e) {{}}
        }}

        // fallback: submit du formulaire
        if (p && p.form) {{
          try {{ p.form.submit(); return true; }} catch(e) {{}}
        }}
        if (p) {{
          try {{
            const ke = new KeyboardEvent('keydown', {{ key: 'Enter', code: 'Enter', which: 13, keyCode: 13, bubbles: true }});
            p.dispatchEvent(ke);
          }} catch(e) {{}}
        }}
        return true;
      }}

      function attempt() {{
        const doc = findFormDoc();
        fillAndSubmit(doc);
      }}

      // tente plusieurs fois (la page peut charger du contenu dynamiquement)
      let tries = 0;
      const timer = setInterval(() => {{
        tries++;
        attempt();
        if (tries >= 8) clearInterval(timer);
      }}, 900);
      setTimeout(attempt, 1200);
      setTimeout(attempt, 2500);
    }})();
  "#, user_json = serde_json::to_string(&username).unwrap(), pass_json = serde_json::to_string(&password).unwrap());

  window.eval(&js).map_err(|e| e.to_string())?;

  // Poll: on essaye de récupérer des cookies de session et de valider côté serveur.
  // Si captcha/2FA, l'utilisateur peut finir dans la fenêtre, puis relancer "Vérifier".
  for _ in 0..30 {
    tokio::time::sleep(std::time::Duration::from_millis(900)).await;

    // Tentative de capture des cookies depuis la webview
    let _ = save_ffe_session(app.clone());

    if let Ok(true) = check_ffe_connected(app.clone()).await {
      // On ferme la fenêtre de login pour revenir à l'app
      if let Some(w) = app.get_webview_window("ffe-login") {
        let _ = w.close();
      }
      return Ok(());
    }
  }

// Fallback: on affiche la fenêtre (au cas où un captcha/2FA/consentement bloque l'auth).
let _ = window.set_position(Position::Logical(LogicalPosition { x: 80.0, y: 80.0 }));
let _ = window.show();
let _ = window.set_focus();

Err("ERREUR DE CONNEXION".into())
}



// Utilisable depuis les autres modules (watcher/epreuves) pour injecter le header Cookie.
pub(crate) fn ffe_cookie_header() -> Option<String> {
  FFE_COOKIES
    .get()
    .map(|m| m.lock().unwrap().clone())
    .map(|s| if s.trim().is_empty() { None } else { Some(s) })
    .flatten()
}


// ===================== CHARGEMENT ÉPREUVES =====================

#[derive(Default)]
struct ScrapeState {
  last: Mutex<Option<Vec<epreuves::Epreuve>>>,
}

#[tauri::command]
fn store_epreuves(state: State<ScrapeState>, epreuves: Vec<epreuves::Epreuve>) -> Result<(), String> {
  let mut g = state.last.lock().map_err(|_| "lock".to_string())?;
  *g = Some(epreuves);
  Ok(())
}

#[derive(Default)]
struct ScrapeDebug {
  msg: Mutex<Option<String>>,
  links: Mutex<Option<Vec<epreuves::Epreuve>>>,
}

#[tauri::command]
fn store_debug(state: State<ScrapeDebug>, msg: String) -> Result<(), String> {
  *state.msg.lock().unwrap() = Some(msg);
  Ok(())
}

#[tauri::command]
fn store_links(state: State<ScrapeDebug>, links: Vec<epreuves::Epreuve>) -> Result<(), String> {
  *state.links.lock().unwrap() = Some(links);
  Ok(())
}


#[tauri::command]
#[allow(non_snake_case)]
async fn list_epreuves(
  _app: tauri::AppHandle,
  _state: State<'_, ScrapeState>,
  contestId: String,
) -> Result<Vec<epreuves::Epreuve>, String> {
  let contest_id = contestId.trim();
  if contest_id.is_empty() {
    return Err("N° concours manquant".into());
  }
  let base_url = format!("https://ffecompet.ffe.com/concours/{}", contest_id);

  // ⚠️ IMPORTANT:
  // Les pages externes (https://ffecompet.ffe.com/...) n'ont pas accès à `invoke` dans la WebView.
  // Donc on scrape ici en Rust (HTTP), pas via JS dans une webview.
  //
  // Si plus tard tu veux parser aussi le "60 / 60" pour chaque épreuve, on pourra l'ajouter,
  // mais ici on renvoie la liste des épreuves (1..N) + un URL "tagué" ?watch_epreuve=N.

  let client = reqwest::Client::builder()
    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
    .redirect(reqwest::redirect::Policy::limited(10))
    .build()
    .map_err(|e| format!("HTTP client error: {}", e))?;

  let resp = client
    .get(&base_url)
    .send()
    .await
    .map_err(|e| format!("HTTP error: {}", e))?;

  let status = resp.status();
  let body = resp.text().await.map_err(|e| format!("HTTP read error: {}", e))?;

  if !status.is_success() {
    let snippet = body.chars().take(240).collect::<String>();
    return Err(format!("HTTP {} sur {} — extrait: {}", status, base_url, snippet));
  }

  fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
      .replace("\u{00A0}", " ")
      .replace("&amp;", "&")
      .replace("&quot;", "\"")
      .replace("&#39;", "'")
      .replace("&lt;", "<")
      .replace("&gt;", ">")
  }

  fn strip_html_to_text(html: &str) -> String {
    // HTML -> texte avec conservation des sauts de ligne utiles (tableaux)
    let mut out = String::with_capacity(html.len().min(200_000));
    let mut in_tag = false;
    let mut prev_was_space = false;

    fn is_nl_trigger(tag: &str) -> bool {
      matches!(
        tag,
        "br" | "/br" |
        "p" | "/p" |
        "tr" | "/tr" |
        "td" | "/td" |
        "th" | "/th" |
        "li" | "/li" |
        "div" | "/div" |
        "section" | "/section" |
        "article" | "/article" |
        "h1" | "/h1" | "h2" | "/h2" | "h3" | "/h3"
      )
    }

    let mut tag_buf = String::new();

    for ch in html.chars() {
      if ch == '<' {
        in_tag = true;
        tag_buf.clear();
        continue;
      }
      if in_tag {
        if ch == '>' {
          in_tag = false;

          // garde le "/" des tags fermants, et normalise en minuscule
          let raw = tag_buf.trim().trim_start_matches('!').split_whitespace().next().unwrap_or("");
          let t = raw.to_ascii_lowercase();

          if is_nl_trigger(&t) {
            if !out.ends_with('\n') {
              out.push('\n');
            }
            prev_was_space = false;
          }
          continue;
        } else {
          tag_buf.push(ch);
          continue;
        }
      }

      // texte
      if ch == '\n' || ch == '\r' {
        if !out.ends_with('\n') {
          out.push('\n');
        }
        prev_was_space = false;
      } else if ch.is_whitespace() || ch == '\u{00A0}' {
        if !prev_was_space && !out.ends_with('\n') {
          out.push(' ');
          prev_was_space = true;
        }
      } else {
        out.push(ch);
        prev_was_space = false;
      }
    }

    decode_entities(&out)
  }

  // Fallback 1: parsing "texte" (si le tableau est dans le HTML)
  let _text = strip_html_to_text(&body);

  // Fallback 2 (prioritaire si le HTML est hydraté côté JS):
  // on tente de trouver une URL d'API JSON dans le HTML, on la requête, et on extrait les épreuves.
  fn extract_urls(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0usize;
    while i + 8 < bytes.len() {
      if &bytes[i..i+8] == b"https://" || (i + 7 < bytes.len() && &bytes[i..i+7] == b"http://") {
        let start = i;
        i += 7;
        while i < bytes.len() {
          let b = bytes[i];
          if b == b'"' || b == b'\'' || b == b'<' || b == b'>' || b.is_ascii_whitespace() {
            break;
          }
          i += 1;
        }
        let url = String::from_utf8_lossy(&bytes[start..i]).to_string();
        if !out.contains(&url) {
          out.push(url);
        }
      } else {
        i += 1;
      }
    }
    out
  }

  fn parse_height_cm_from_label(label: &str) -> Option<u32> {
    let start = label.find('(')?;
    let end = label[start..].find('m')?;
    let inside = &label[start+1..start+end];
    let mut digits = String::new();
    for ch in inside.chars() {
      if ch.is_ascii_digit() || ch == ',' || ch == '.' {
        digits.push(ch);
      }
    }
    if digits.is_empty() { return None; }
    let val = digits.replace(',', ".");
    let m: f32 = val.parse().ok()?;
    Some((m * 100.0).round() as u32)
  }

  fn find_epreuves_in_json(v: &serde_json::Value) -> Vec<(u32, String, Option<u32>, Option<u32>, Option<u32>)> {
    let mut out = Vec::new();

    fn get_u32(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<u32> {
      for k in keys {
        if let Some(val) = obj.get(*k) {
          if let Some(n) = val.as_u64() { return Some(n as u32); }
          if let Some(s) = val.as_str() {
            if let Ok(n) = s.trim().parse::<u32>() { return Some(n); }
          }
        }
      }
      None
    }

    fn get_str(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
      for k in keys {
        if let Some(val) = obj.get(*k) {
          if let Some(s) = val.as_str() {
            let s = s.trim();
            if !s.is_empty() { return Some(s.to_string()); }
          }
        }
      }
      None
    }

    fn walk(v: &serde_json::Value, out: &mut Vec<(u32, String, Option<u32>, Option<u32>, Option<u32>)>) {
      match v {
        serde_json::Value::Object(obj) => {
          if let Some(arr) = obj.get("epreuves").and_then(|x| x.as_array()) {
            for it in arr { walk(it, out); }
          }

          let num = get_u32(obj, &["numEpreuve","numeroEpreuve","numero","num","ordre"]);
          let label = get_str(obj, &["libelle","label","nom","intitule","designation"]);
          if let (Some(n), Some(lab)) = (num, label) {
            let disc = get_str(obj, &["discipline","codeDiscipline","disc"]);
            if disc.as_deref().unwrap_or("SO") == "SO" || disc.is_none() {
              let engaged = get_u32(obj, &["nbEngages","engages","nbEngage","nbPartants","participants"]);
              let max = get_u32(obj, &["nbMax","maxEngages","nbMaxEngages","max","capacite"]);
              let height = parse_height_cm_from_label(&lab);
              out.push((n, lab, height, engaged, max));
            }
          }

          for (_k, vv) in obj.iter() { walk(vv, out); }
        }
        serde_json::Value::Array(arr) => {
          for it in arr { walk(it, out); }
        }
        _ => {}
      }
    }

    walk(v, &mut out);
    out
  }

  

fn parse_ratio(line: &str) -> Option<(u32, u32)> {
  // Cherche un motif "X / Y" n'importe où dans la chaîne
  let mut nums: Vec<u32> = Vec::new();
  let mut cur = String::new();
  for ch in line.chars() {
    if ch.is_ascii_digit() {
      cur.push(ch);
    } else {
      if !cur.is_empty() {
        if let Ok(v) = cur.parse::<u32>() {
          nums.push(v);
          if nums.len() >= 2 { break; }
        }
        cur.clear();
      }
    }
  }
  if !cur.is_empty() && nums.len() < 2 {
    if let Ok(v) = cur.parse::<u32>() { nums.push(v); }
  }
  if nums.len() >= 2 { Some((nums[0], nums[1])) } else { None }
}

fn try_parse_json_epreuves(body_text: &str) -> Vec<(u32, String, Option<u32>, Option<u32>, Option<u32>)> {
  // Retourne: (num, libellé, hauteur_cm, engagés, max)
  let mut out: Vec<(u32, String, Option<u32>, Option<u32>, Option<u32>)> = Vec::new();

  let v: serde_json::Value = match serde_json::from_str(body_text) {
    Ok(v) => v,
    Err(_) => return out,
  };

  fn height_cm_from_label(label: &str) -> Option<u32> {
    // Ex: "(1,10 m)" -> 110
    let start = label.find('(')?;
    let end_rel = label[start..].find('m')?;
    let chunk = &label[start + 1..start + end_rel];
    let chunk = chunk.trim().replace(',', ".");
    let filtered: String = chunk.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let m = filtered.parse::<f32>().ok()?;
    Some((m * 100.0).round() as u32)
  }

  fn walk(val: &serde_json::Value, out: &mut Vec<(u32, String, Option<u32>, Option<u32>, Option<u32>)>) {
    match val {
      serde_json::Value::Array(arr) => for x in arr { walk(x, out); },
      serde_json::Value::Object(map) => {
        let num = map.get("numero")
          .or_else(|| map.get("num"))
          .or_else(|| map.get("no"))
          .and_then(|x| x.as_u64())
          .map(|x| x as u32);

        let label = map.get("libelle")
          .or_else(|| map.get("label"))
          .or_else(|| map.get("nom"))
          .and_then(|x| x.as_str())
          .map(|s| s.to_string());

        let engages = map.get("engages")
          .or_else(|| map.get("engaged"))
          .or_else(|| map.get("nbEngages"))
          .and_then(|x| x.as_u64())
          .map(|x| x as u32);

        let max = map.get("max")
          .or_else(|| map.get("capacite"))
          .or_else(|| map.get("nbMax"))
          .and_then(|x| x.as_u64())
          .map(|x| x as u32);

        if let (Some(n), Some(lab)) = (num, label) {
          let h = height_cm_from_label(&lab);
          out.push((n, lab, h, engages, max));
        }

        for (_k, v) in map.iter() { walk(v, out); }
      }
      _ => {}
    }
  }

  walk(&v, &mut out);
  out.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
  out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
  out
}

async fn try_api_extract(
  client: &reqwest::Client,
  urls: Vec<String>,
  contest_id: &str
) -> Option<Vec<(u32, String, Option<u32>, Option<u32>, Option<u32>)>> {
  for url in urls {
    let lu = url.to_lowercase();
    // heuristique: ne tester que les urls qui ressemblent à de l'API/JSON
    if !(lu.contains("api") || lu.contains("json") || lu.contains("data")) { continue; }
    // souvent l'id concours est présent dans l'URL d'API
    if !lu.contains(contest_id) { continue; }

    let resp = client.get(&url).send().await.ok()?;

    // lire ce dont on a besoin AVANT de consommer resp
    let ct: String = resp
      .headers()
      .get(reqwest::header::CONTENT_TYPE)
      .and_then(|v| v.to_str().ok())
      .unwrap_or("")
      .to_string();

    // consommer resp une seule fois
    let body_text = resp.text().await.ok()?;

    let looks_json =
      ct.contains("json")
      || body_text.trim_start().starts_with('{')
      || body_text.trim_start().starts_with('[');

    if !looks_json { continue; }

    let items = try_parse_json_epreuves(&body_text);
    if !items.is_empty() { return Some(items); }
  }
  None
}

  // 1) Tentative via API JSON détectée dans le HTML (souvent le cas si le tableau est hydraté en JS)
  let urls = extract_urls(&body);
  let mut from_api: Option<Vec<(u32, String, Option<u32>, Option<u32>, Option<u32>)>> = None;
  if !urls.is_empty() {
    from_api = try_api_extract(&client, urls, contest_id).await;
  }

  let text = strip_html_to_text(&body);

  // 2) Fallback via texte "visible" si le tableau est déjà dans le HTML
  let mut found: std::collections::BTreeMap<u32, (String, Option<u32>, Option<u32>, Option<u32>)> = std::collections::BTreeMap::new();

  if let Some(items) = from_api {
    for (num, label, height_cm, engaged, max) in items {
      found.entry(num).or_insert((label, height_cm, engaged, max));
    }

} else {  // Parsing robuste par TOKENS (le tableau peut être éclaté en cellules: "1" puis "SO" sur la ligne suivante, etc.)
  // On tokenise tout le texte et on reconstruit les blocs: [num][SO][label ... ,][... X / Y ...]
  let tokens: Vec<&str> = text.split_whitespace().collect();
  let mut i: usize = 0;

  while i < tokens.len() {
    // Détecter début d'épreuve: <num> SO
    let num_opt = tokens[i].parse::<u32>().ok();
    if let (Some(num), Some(next)) = (num_opt, tokens.get(i + 1).copied()) {
      if next == "SO" {
        i += 2;

        // Collecter le label jusqu'à une virgule finale (souvent ",") ou jusqu'au prochain début d'épreuve
        let mut label_parts: Vec<&str> = Vec::new();
        while i < tokens.len() {
          // Stop si prochain début d'épreuve
          if tokens[i].parse::<u32>().is_ok() && tokens.get(i + 1).copied() == Some("SO") {
            break;
          }
          let t = tokens[i];
          // Virgule terminale: soit token == "," soit token finit par ","
          if t == "," {
            i += 1;
            break;
          }
          if t.ends_with(',') {
            label_parts.push(&t[..t.len() - 1]);
            i += 1;
            break;
          }
          label_parts.push(t);
          i += 1;
        }

        let label = label_parts.join(" ").trim().to_string();
        if !label.is_empty() {
          let height_cm = parse_height_cm_from_label(&label);
          found.entry(num).or_insert((label, height_cm, None, None));
        }

        // Scanner ensuite pour un ratio X / Y avant la prochaine épreuve
        while i + 2 < tokens.len() {
          if tokens[i].parse::<u32>().is_ok() && tokens.get(i + 1).copied() == Some("SO") {
            break; // prochaine épreuve
          }
          if let (Ok(e), Some("/"), Ok(mmax)) = (
            tokens[i].parse::<u32>(),
            tokens.get(i + 1).copied(),
            tokens[i + 2].parse::<u32>()
          ) {
            if let Some(entry) = found.get_mut(&num) {
              entry.2 = Some(e);
              entry.3 = Some(mmax);
            }
            i += 3;
            break;
          }
          i += 1;
        }

        continue;
      }
    }

    i += 1;
  }
}


  let mut epreuves_out: Vec<epreuves::Epreuve> = Vec::new();
  for (num, (label, height_cm, engaged, max)) in found {
    let url = format!("{}?watch_epreuve={}", base_url, num);

    // Label demandé: "Epreuve 1 110 60/60" (si info dispo)
    let mut parts = vec![format!("Epreuve {}", num)];
    if let Some(h) = height_cm { parts.push(format!("{}", h)); }
    if let (Some(e), Some(m)) = (engaged, max) { parts.push(format!("{}/{}", e, m)); }

    // fallback si pas de ratio/hauteur
    if parts.len() == 1 {
      parts.push(label.clone());
    }

    epreuves_out.push(epreuves::Epreuve { label: parts.join(" "), url });
  }

  if epreuves_out.is_empty() {
    // debug utile: on renvoie quelques lignes qui contiennent "SO" pour comprendre le format réel
    let mut samples: Vec<String> = Vec::new();
    // lignes texte
    for raw in text.lines() {
      let l = raw.trim();
      if l.contains("SO") {
        samples.push(l.chars().take(220).collect());
      }
      if samples.len() >= 8 { break; }
    }
    // si vide, renvoyer aussi quelques urls détectées
    if samples.is_empty() {
      samples.push(format!("urls_detected={}", extract_urls(&body).len()));
    }
    return Err(format!(
      "0 épreuve détectée (scrape Rust) — url={} — samples={:?}",
      base_url, samples
    ));
  }

  Ok(epreuves_out)
}


// ===============================================================


fn main() {
  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .manage(ScrapeState::default())
    .manage(ScrapeDebug::default())
    .manage(AppState {
      watcher: Arc::new(Watcher::new()),
      handle: Mutex::new(None),
      db_path: Mutex::new(None),
    })
    .invoke_handler(tauri::generate_handler![
      list_targets,
      add_target,
      delete_target,
      start_watcher,
      stop_watcher,
      open_url,
      store_epreuves,
      list_epreuves,
      store_debug,
      store_links,
      set_login_debug,
      get_login_debug,
      set_probe,
      save_ffe_session,
      is_ffe_connected,
      check_ffe_connected,
      ffe_login_with_credentials
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
