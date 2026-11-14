# Installateur du Liberl News

Les patchs de traduction française du Liberl News, en un seul exécutable : Windows
(`InstallateurLiberlNews.exe`) et Linux / Steam Deck (`InstallateurLiberlNews-linux`).

- **Trails in the Sky FC, SC, the 3rd** : détecte le jeu (Steam, GOG), installe le patch
  publié sur la plateforme (deltas xdelta3 appliqués aux fichiers anglais, gardés dans
  `dat_en`, chaque fichier vérifié par SHA-256), le met à jour, le désinstalle. Les voix
  japonaises (SoraVoice : `dsound.dll` + `voice/`) viennent de la release « voix » du dépôt
  du jeu ; sous Linux, le préfixe Proton reçoit `dsound=native,builtin`.
- **Jeux rétro PC-88** : traduit les disquettes du joueur (copie « [FR] » à côté de
  l'original).
- **Créations** (Dolce Traccia) : ouvertes dans le navigateur.

Le catalogue, les articles et les bandes-annonces viennent du site
(`api/launcher.php`, `trailer.php`) ; les thèmes de chaque projet de `assets/themes.json`.

## Compiler

```
cargo build --release
```

La CI (`.github/workflows/build.yml`) produit les deux exécutables ; un tag `v…` les publie
en release. Sous Linux, `assets/xdelta3-linux` est copié depuis Ubuntu par la CI.

## Essayer sans toucher au vrai jeu

- `LN_GAME_DIR` : dossier d'une copie du jeu ;
- `LN_PATCH_DIR` + `LN_PATCH_PROJECT` : dossier d'une release locale (patch.json + deltas) ;
- `LN_LAUNCHER_URL`, `LN_TRAILER_URL` : un site local.

## Voix

`scripts/pack_voices.py` découpe un dossier `voice/` en zips de 1,5 Go (limite GitHub :
2 Go par fichier), à publier en release « voix » du dépôt du jeu.

L'ancien installateur (Electron) reste dans l'historique de ce dépôt.
