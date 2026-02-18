use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};
use tokio::time::{sleep, Duration};
use rusqlite::Connection;
use crate::{db, detect::{detect_status_from_html, detect_slots_from_html, Status}};

pub struct Watcher {
  running: Arc<AtomicBool>,
}

impl Watcher {
  pub fn new() -> Self {
    Self { running: Arc::new(AtomicBool::new(false)) }
  }

  pub fn is_running(&self) -> bool {
    self.running.load(Ordering::SeqCst)
  }

  pub fn stop(&self) {
    self.running.store(false, Ordering::SeqCst);
  }

  pub async fn run_loop(
    &self,
    db_path: std::path::PathBuf,
    notify: Arc<dyn Fn(i64, String, String) + Send + Sync>,
  ) {
    self.running.store(true, Ordering::SeqCst);

    let client = match reqwest::Client::builder()
      .user_agent("FFE Watcher (Tauri) - pre-engagement assisted")
      .build()
    {
      Ok(c) => c,
      Err(_) => return,
    };

    while self.running.load(Ordering::SeqCst) {
      let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => { sleep(Duration::from_secs(2)).await; continue; }
      };

      let targets = match db::list_targets(&conn) {
        Ok(t) => t,
        Err(_) => { sleep(Duration::from_secs(2)).await; continue; }
      };
      drop(conn);

      if targets.is_empty() {
        sleep(Duration::from_secs(2)).await;
        continue;
      }

      for t in targets {
        if !self.running.load(Ordering::SeqCst) { break; }

        let interval = t.interval_normal_sec.max(15);
        let now = chrono::Utc::now().timestamp();

        let mut status;
        let mut err: Option<String> = None;

        // ✅ On garde le HTML pour extraire ensuite "52 / 60" → places restantes
        let mut html_opt: Option<String> = None;

        let mut req = client.get(&t.url);
        if let Some(c) = crate::ffe_cookie_header() {
          req = req.header("Cookie", c);
        }

        match req.send().await {
          Ok(resp) => {
            if !resp.status().is_success() {
              status = Status::Error;
              err = Some(format!("HTTP {}", resp.status()));
            } else {
              match resp.text().await {
                Ok(html) => {
                  status = detect_status_from_html(&html);
                  html_opt = Some(html);
                }
                Err(e) => { status = Status::Error; err = Some(format!("read body: {e}")); }
              }
            }
          }
          Err(e) => { status = Status::Error; err = Some(format!("http: {e}")); }
        }

        // double-confirm if OPEN
        if status == Status::Open {
          sleep(Duration::from_secs(1)).await;
          let mut req2 = client.get(&t.url);
          if let Some(c) = crate::ffe_cookie_header() {
            req2 = req2.header("Cookie", c);
          }
          if let Ok(resp2) = req2.send().await {
            if let Ok(html2) = resp2.text().await {
              let s2 = detect_status_from_html(&html2);
              if s2 != Status::Open { status = s2; }
              // on remplace par le HTML le plus récent
              html_opt = Some(html2);
            }
          }
        }

        let status_str = match status {
          Status::Unknown => "UNKNOWN",
          Status::Closed => "CLOSED",
          Status::Open => "OPEN",
          Status::Full => "FULL",
          Status::Error => "ERROR",
        }.to_string();

        let old = t.last_status.clone();

        if let Ok(conn2) = Connection::open(&db_path) {
          let _ = db::update_status(&conn2, t.id, &status_str, now, err.as_deref());

          // ✅ Alerte "place libérée" : FULL (0) → >0
          if let Some(html) = html_opt.as_deref() {
            if let Some(slots_now) = detect_slots_from_html(html) {
              let last_slots = t.last_slots.unwrap_or(-1);

              if last_slots == 0 && slots_now > 0 {
                notify(t.id, t.label.clone(), t.url.clone());
              }

              let _ = db::set_last_slots(&conn2, t.id, slots_now);
            }
          }
        }

        // 🚨 Alerte "ouverture engagements"
        if old != "OPEN" && status_str == "OPEN" {
          notify(t.id, t.label.clone(), t.url.clone());
        }

        sleep(Duration::from_millis(250)).await;
        sleep(Duration::from_secs(interval as u64)).await;
      }
    }
  }
}
