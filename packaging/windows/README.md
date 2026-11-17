# Passage de l’ancien Setup Electron au nouvel installateur

Le paquet NSIS conserve l’identité de la version 1.2.1 :

- appId : `com.liberlnews.installer` ;
- GUID de désinstallation : `4b43ab33-61ca-59c6-b165-8140fc87bcc9` ;
- nom de paquet / dossier d’installation : `setupliberlnews` ;
- exécutable et raccourcis : `Installateur du Liberl News.exe`.

Le contenu installé est le binaire Rust, renommé pour conserver les raccourcis,
et l’assistant d’élévation NSIS. Aucun moteur Electron n’est embarqué. Les données
utilisateur sont conservées ; les dossiers des jeux ne font pas partie du Setup.

## Génération locale

Depuis la racine du dépôt :

```
npm ci --prefix packaging/windows
cargo build --release
node scripts/package-windows.cjs
node scripts/test-windows-bridge.cjs
```

`--exe chemin.exe` permet d’emballer et tester une copie d’un binaire optimisé.
Les résultats sont dans `target/windows-setup/` : Setup, blockmap et `latest.yml`.
La version est lue dans Cargo.toml. Le script utilise toujours `--publish never`.

## Publication future

Le workflow construit et teste le Setup à chaque compilation Windows. Seul un tag
`v<version de Cargo.toml>` publie les fichiers : l’exécutable Windows portable, le
binaire Linux, le Setup compatible Electron, son blockmap et `latest.yml`.
La première version publique prévue est `v2.0.0`, supérieure à l’ancienne 1.2.1.
Il faut augmenter Cargo.toml avant chaque nouvelle version publique native.

L’ancien Setup télécharge la mise à jour à l’ouverture et lance son installation
à la fermeture. Le lancement suivant utilise le nouvel installateur. L’ancien
portable demande toujours un téléchargement manuel. Le nouvel installateur natif
propose ensuite ses propres mises à jour à partir du binaire portable de la release.

## Vérification et limite du test

Le test local utilise electron-updater 6.8.9, comme l’ancien projet, avec une
application simulée en version 1.2.1 et un serveur HTTP local. Il vérifie la
détection, le téléchargement, le SHA-512, le déclenchement du Setup à la fermeture,
le contenu natif extrait et l’identité NSIS. Il n’exécute pas le Setup et ne modifie
aucune installation Windows. La mise à niveau réelle d’une installation 1.2.1,
ses raccourcis et sa désinstallation restent à valider dans une VM Windows avant
diffusion publique.
