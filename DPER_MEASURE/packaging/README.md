# Livraison Windows — DPM-31

## Construire une version précise

Prérequis de la machine de build : Windows x64, Git, Rust/Cargo/rustup, cible
`x86_64-pc-windows-msvc`, outils de compilation MSVC et Inno Setup 6 avec
`ISCC.exe` dans PATH. Ces outils ne sont pas installés automatiquement.

Le dossier DPER_MEASURE doit être propre et HEAD doit correspondre au commit
choisi. Le script ne change pas de branche et ne supprime aucune modification.
Les modifications d'autres outils du dépôt sont hors périmètre du contrôle.

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
$versionSource = git rev-parse HEAD
.\packaging\build-windows-installer.ps1 -SourceCommit $versionSource
```

La version affichée provient de `[package].version` dans Cargo.toml. Le build
impose la cible MSVC x64 et le dossier `dist/packaging-build`, même si
`CARGO_TARGET_DIR` désigne un autre emplacement. Il utilise Cargo.lock avec
`--locked`, vérifie l'en-tête PE du binaire puis prépare une copie des quatre
fichiers autorisés. Inno ne lit plus un ancien binaire de `target/release`.

Sortie : `dist/installer/<version>_<12 premiers caractères du commit>/`, contenant
l'installateur et `manifest.json`. Celui-ci conserve le commit complet, la version,
la cible, la version rustc, l'empreinte de Cargo.lock, les quatre fichiers et
l'empreinte de l'installateur. Une livraison existante n'est pas écrasée.
Ce processus assure une sélection et une traçabilité des sources ; il ne
garantit pas une identité binaire entre deux environnements de compilation.

Les seules entrées distribuées sont `dper_measure.exe`, `LICENSE.txt`,
`NOTICE_UTILISATEUR.md` et `assets/demo_calibration_virtuelle.png`. L'atlas des
icônes est embarqué dans l'exécutable. Les captures, projets, PDF de travail,
autres assets et secrets locaux ne sont pas copiés. La notice décrit les outils
PDF/OCR/vidéo externes et le runtime Visual C++ x64 requis par l'application.

Les builds, préparations et sorties restent sous `dist/`, déjà ignoré par Git.
En cas d'échec, les préparations ne sont pas publiées et restent inspectables.
Les sorties ne sont ni signées ni téléchargées/publiées automatiquement.

## Vérifications ciblées

```powershell
.\packaging\test-windows-packaging.ps1
```

Onze contrôles vérifient la syntaxe, la version package, le commit, les fichiers
modifiés/non suivis, la liste fermée, les fichiers manquants/vides, les en-têtes
x64/x86, les échecs Cargo/Inno, l'absence de sortie et les empreintes/publication.
Ils créent leur propre dépôt **synthétique** sous `output/dpm31-packaging-tests/`.
Ils ne créent aucun worktree et ne modifient pas l'index du dépôt de travail.
La compilation Cargo et Inno sont simulées pour tester l'orchestration : le
fichier `.exe` produit par ces doubles n'est pas un installateur utilisable.

## Résultats du 7 novembre 2026

Base `ddeb4a7cb46acd286c2d4f121ef5612f535f32b4` (PR #39 fusionnée).
Onze contrôles réussis après correction de la portée des variables des doubles.
La relecture a aussi trouvé et corrigé `CloseApplications=ask`, valeur Inno
invalide, remplacée par `yes`.

Lors du premier contrôle, le script de production a refusé « Prérequis absent :
ISCC.exe », sans lancer de build. La vérification des sources a également été
appelée sur le dépôt réel et a refusé ses modifications locales, préservées.
La modification préexistante de Cargo.lock demeure hors de cette livraison.

### Compilation réelle après installation d'Inno Setup

Inno Setup 6.7.3 a été retrouvé dans `N:\TOOLS\INNO\Inno Setup 6` et ajouté au
PATH du processus de build. La seule modification locale de DPER_MEASURE était
une ligne vide finale supprimée de Cargo.lock : elle a été enregistrée dans un
commit distinct, sans changement de dépendance. Les fichiers recovery hors de
l'outil sont restés hors commit.

Commande réellement exécutée :

```powershell
$env:Path += ';N:\TOOLS\INNO\Inno Setup 6'
.\packaging\build-windows-installer.ps1 -SourceCommit 4c7e70248c255443714794c8b6c34587e192a355
```

- Compilation Release x64 réussie en 6 min 09 s, rustc 1.92.0 ; 37 avertissements
  de code inutilisé signalés.
- Compilation Inno réelle réussie en 3,844 s. Un avertissement : l'identifiant
  d'architecture `x64` est obsolète et substitué par `x64os` par cette version.
- Installateur réel :
  `dist/installer/0.2.0_4c7e70248c25/DPER_Measure_Setup_0.2.0_x64.exe`,
  7 972 330 octets. Manifeste `manifest.json` dans le même dossier.
- SHA-256 de l'installateur, vérifié après publication locale :
  `0EAFD8A2A5E4D2F8F3DC76DBD1C8644116923C71E8C1914C71C421E32624AED2`.
  Les empreintes des quatre fichiers préparés ont également été revérifiées.
- Journal local : `output/dpm31-real-build.log`.

Aucune suite Rust ni installation réelle n'a été exécutée pour ce ticket.
L'installation, le lancement, la mise à jour et la désinstallation ci-dessous
restent à vérifier avant la clôture de DPM-31. L'installateur n'est pas signé.

## Validation finale sur Windows

1. Installer les prérequis sur la machine de build et sélectionner le commit
   approuvé dans un dossier propre, après sauvegarde/livraison des modifications
   locales utiles. Ne pas effacer les changements locaux pour contourner le refus.
2. Exécuter le script de production. Vérifier le commit, la version et les SHA-256
   du manifeste, puis le contenu exact des quatre fichiers installés.
3. Sur une machine Windows x64 de test sans runtime Visual C++, vérifier le
   message de refus. Installer le runtime puis reprendre l'installation.
4. Lancer l'application depuis le menu Démarrer. Vérifier les icônes et l'ouverture
   de la mire par le parcours guidé. Sans outils PDF/OCR/vidéo, vérifier les messages
   d'indisponibilité ; après leur installation, essayer une fonction de chaque type.
5. Sauvegarder un projet synthétique dans Documents. Tester la mise à jour et la
   désinstallation sans perdre ce projet. Consigner les versions Windows/Inno/Rust,
   les résultats et le manifeste dans le ticket et la PR.

Ces essais valident la distribution logicielle ; la qualification métrologique
sur pièces physiques reste dans DPM-26.
