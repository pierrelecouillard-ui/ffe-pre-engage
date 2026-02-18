use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
  pub id: i64,
  pub label: String,
  pub url: String,
  pub cheval: Option<String>,
  pub cavalier: Option<String>,
  pub interval_normal_sec: i64,
  pub interval_hot_sec: i64,
  pub hot_from: Option<String>,
  pub hot_to: Option<String>,
  pub last_status: String,
  pub last_checked_at: Option<i64>,
  pub last_error: Option<String>,
  pub last_slots: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTargetPayload {
  pub label: String,
  pub url: String,
  pub cheval: Option<String>,
  pub cavalier: Option<String>,
  pub interval_normal_sec: Option<i64>,
  pub interval_hot_sec: Option<i64>,
  pub hot_from: Option<String>,
  pub hot_to: Option<String>,
}
