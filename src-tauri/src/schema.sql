CREATE TABLE IF NOT EXISTS targets (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  label TEXT NOT NULL,
  url TEXT NOT NULL,
  cheval TEXT,
  cavalier TEXT,
  interval_normal_sec INTEGER NOT NULL DEFAULT 300,
  interval_hot_sec INTEGER NOT NULL DEFAULT 45,
  hot_from TEXT,
  hot_to TEXT,
  last_status TEXT NOT NULL DEFAULT 'UNKNOWN',
  last_checked_at INTEGER,
  last_change_at INTEGER,
  last_error TEXT
);

CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  target_id INTEGER NOT NULL,
  ts INTEGER NOT NULL,
  status TEXT NOT NULL,
  note TEXT,
  FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE
);
