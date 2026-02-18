# FFE Pre-Engagement Assisté (Tauri v2)

Squelette **Option A++** :
- Watchlist (cibles)
- Polling "soft" + double-confirmation
- Statut CLOSED/OPEN/FULL/...
- Notification OS quand ça passe OPEN
- Bouton "Ouvrir" (ouvre l'URL dans le navigateur)
- Panneau "Sprint" (checklist + copier)

⚠️ Ce projet **n'automatise pas** la connexion ni l'engagement.

## Démarrage
```bash
npm install
npm run tauri dev
```

## Adapter la détection
Le détecteur OPEN/FULL/CLOSED est dans `src-tauri/src/detect.rs`.
Pour une détection fiable à 100%, adapte les mots-clés/sélecteurs à ta page FFE.
