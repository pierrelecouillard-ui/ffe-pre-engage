use regex::Regex;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Epreuve {
  pub label: String,
  pub url: String,
}

// Parsing simple: on cherche des liens vers des pages d’épreuves dans le HTML.
#[allow(dead_code)]
pub fn parse_epreuves(html: &str) -> Vec<Epreuve> {
  // capture href + texte du lien
  // Exemple: <a href="/epreuve/....">Épreuve 110cm ...</a>
  let re = Regex::new(r#"<a[^>]+href="([^"]+)"[^>]*>([^<]+)</a>"#).unwrap();

  let mut out = Vec::new();

  for cap in re.captures_iter(html) {
    let href = cap.get(1).unwrap().as_str().trim().to_string();
    let text = cap.get(2).unwrap().as_str().trim().to_string();

    // Filtre : adapte si besoin selon les URL réelles FFE
    if href.contains("epreuve") || href.contains("epreuves") {
      let full = if href.starts_with("http") {
        href
      } else {
        format!("https://ffecompet.ffe.com{}", href)
      };

      // évite les doublons
      if !out.iter().any(|e: &Epreuve| e.url == full) {
        out.push(Epreuve { label: text, url: full });
      }
    }
  }

  out
}
