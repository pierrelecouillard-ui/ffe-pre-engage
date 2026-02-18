use rusqlite::{params, Connection};
use crate::models::{AddTargetPayload, Target};

pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
  conn.execute_batch(include_str!("schema.sql"))?;

  // Migration légère: ajout de last_slots si absent
  // (SQLite n'a pas IF NOT EXISTS sur ADD COLUMN dans toutes les versions)
  let _ = conn.execute("ALTER TABLE targets ADD COLUMN last_slots INTEGER", []);

  Ok(())
}

pub fn db_path(app_data_dir: &std::path::Path) -> std::path::PathBuf {
  app_data_dir.join("ffe_pre_engage.sqlite")
}

pub fn add_target(conn: &Connection, p: AddTargetPayload) -> rusqlite::Result<()> {
  let interval_normal = p.interval_normal_sec.unwrap_or(300).max(15);
  let interval_hot = p.interval_hot_sec.unwrap_or(45).max(10);

  conn.execute(
    r#"INSERT INTO targets(label,url,cheval,cavalier,interval_normal_sec,interval_hot_sec,hot_from,hot_to)
       VALUES(?,?,?,?,?,?,?,?)"#,
    params![p.label, p.url, p.cheval, p.cavalier, interval_normal, interval_hot, p.hot_from, p.hot_to],
  )?;
  Ok(())
}

pub fn delete_target(conn: &Connection, id: i64) -> rusqlite::Result<()> {
  // IMPORTANT: supprimer d'abord les enfants
  conn.execute("DELETE FROM events WHERE target_id=?", params![id])?;
  conn.execute("DELETE FROM targets WHERE id=?", params![id])?;
  Ok(())
}

pub fn list_targets(conn: &Connection) -> rusqlite::Result<Vec<Target>> {
  let mut stmt = conn.prepare(
    "SELECT id,label,url,cheval,cavalier,interval_normal_sec,interval_hot_sec,hot_from,hot_to,last_status,last_checked_at,last_error,last_slots
     FROM targets ORDER BY id DESC"
  )?;
  let rows = stmt.query_map([], |r| {
    Ok(Target{
      id: r.get(0)?,
      label: r.get(1)?,
      url: r.get(2)?,
      cheval: r.get(3)?,
      cavalier: r.get(4)?,
      interval_normal_sec: r.get(5)?,
      interval_hot_sec: r.get(6)?,
      hot_from: r.get(7)?,
      hot_to: r.get(8)?,
      last_status: r.get(9)?,
      last_checked_at: r.get(10)?,
      last_error: r.get(11)?,
      last_slots: r.get(12)?,
    })
  })?;

  let mut out = Vec::new();
  for row in rows { out.push(row?); }
  Ok(out)
}

pub fn update_status(conn: &Connection, id: i64, status: &str, now: i64, err: Option<&str>) -> rusqlite::Result<()> {
  conn.execute(
    "UPDATE targets SET last_status=?, last_checked_at=?, last_change_at=CASE WHEN last_status<>? THEN ? ELSE last_change_at END, last_error=? WHERE id=?",
    params![status, now, status, now, err, id]
  )?;

  conn.execute(
    "INSERT INTO events(target_id, ts, status, note) VALUES(?,?,?,?)",
    params![id, now, status, err.unwrap_or("")]
  )?;
  Ok(())
}

pub fn set_last_slots(conn: &Connection, id: i64, slots: i32) -> rusqlite::Result<()> {
  conn.execute(
    "UPDATE targets SET last_slots=? WHERE id=?",
    params![slots, id]
  )?;
  Ok(())
}
