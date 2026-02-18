#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
  Unknown,
  Closed,
  Open,
  Full,
  Error,
}

pub fn detect_status_from_html(html: &str) -> Status {
  let h = html.to_lowercase();

  // FULL first
  let full_keys = ["complet", "complète", "plus de place", "liste d'attente", "full"];
  if full_keys.iter().any(|k| h.contains(k)) {
    return Status::Full;
  }

  // OPEN
  let open_keys = ["engager", "engagement ouvert", "ouvert", "inscription ouverte", "inscriptions ouvertes"];
  if open_keys.iter().any(|k| h.contains(k)) {
    return Status::Open;
  }

  // CLOSED
  let closed_keys = ["engagement fermé", "fermé", "ouverture le", "ouvre le", "pas encore ouvert"];
  if closed_keys.iter().any(|k| h.contains(k)) {
    return Status::Closed;
  }

  Status::Unknown
}

/// Détecte le nombre de places restantes à partir d'un motif de type `52 / 60`
/// (souvent affiché comme "engagés 52 / 60").
/// Retourne `Some(slots_restants)` si on trouve un motif, sinon `None`.
pub fn detect_slots_from_html(html: &str) -> Option<i32> {
  let h = html.to_lowercase();

  // On essaie d'abord de chercher un motif après le mot "engag" (engagé/engagés/engagements)
  if let Some(idx) = h.find("engag") {
    if let Some(v) = parse_first_ratio(&h[idx..]) {
      return Some(v);
    }
  }

  // Sinon, on prend le premier motif X/Y du document
  parse_first_ratio(&h)
}

/// Parse le premier motif `X / Y` dans un texte, et renvoie `Y - X` (>= 0).
fn parse_first_ratio(s: &str) -> Option<i32> {
  let bytes = s.as_bytes();
  let n = bytes.len();
  let mut i = 0;

  while i < n {
    // lire un nombre X
    if !bytes[i].is_ascii_digit() {
      i += 1;
      continue;
    }
    let (x, j) = read_int(bytes, i)?;
    i = j;

    // espaces
    while i < n && bytes[i].is_ascii_whitespace() { i += 1; }

    // slash
    if i >= n || bytes[i] != b'/' { continue; }
    i += 1;

    // espaces
    while i < n && bytes[i].is_ascii_whitespace() { i += 1; }

    // lire un nombre Y
    if i >= n || !bytes[i].is_ascii_digit() { continue; }
    let (y, k) = read_int(bytes, i)?;
    i = k;

    // sanity: Y doit être >= X et Y raisonnable
    if y >= x {
      let slots = y - x;
      return Some(slots);
    }
  }

  None
}

fn read_int(bytes: &[u8], start: usize) -> Option<(i32, usize)> {
  let mut i = start;
  let mut val: i64 = 0;
  while i < bytes.len() && bytes[i].is_ascii_digit() {
    val = val * 10 + (bytes[i] - b'0') as i64;
    // évite les overflow absurdes
    if val > i32::MAX as i64 { return None; }
    i += 1;
  }
  Some((val as i32, i))
}
