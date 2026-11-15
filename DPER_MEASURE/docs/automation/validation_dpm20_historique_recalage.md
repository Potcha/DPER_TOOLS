# DPM-20 — Conserver les preuves après correction du recalage

La modification d’un recalage conservait les mesures natives mais supprimait
leurs observations d’inspection. Le traitement commun aux interfaces native et
MCP conserve désormais ces observations comme historique. Les profils, points,
réglages, occurrences, valeurs et liens vers les mesures restent disponibles.
L’autorité courante des observations et des décisions est retirée. Le champ
existant `imported` identifie aussi ces preuves devenues historiques.

Une nouvelle observation et son enregistrement explicite sont nécessaires.
Restaurer les coordonnées du recalage précédent ne réactive pas ses preuves.
Les mesures d’occurrence et leurs descendants restent périmés. Les anciens
aperçus ne permettent plus d’enregistrement. Le schéma de stockage est conservé.
Les limites existantes de rétention des observations restent applicables.

## Vérifications exécutées le 15 novembre 2026

- `cargo test --locked inspection_observation_ui::tests::` — 23 réussites.
  Le nouveau test crée une observation, une base de décision revue et un
  descendant natif. Il corrige puis restaure le recalage, compare les preuves
  conservées, vérifie l’invalidation permanente et l’absence d’observation
  courante, refuse la réutilisation de l’ancien aperçu, puis enregistre une
  nouvelle observation distincte et vérifie la sérialisation.
- `cargo test --locked inspection_plan_ui::tests::` — 21 réussites et un
  échec d’installation de PDF dans le dossier temporaire restreint, erreur
  Windows 650. Le test précis
  `inspection_plan_ui::tests::acceptance::composed_native_workflow_decision_independent_mcp_replay_save_reopen_and_exports`
  a ensuite réussi hors de cette restriction. Soit 45 tests distincts réussis.
- `cargo build --locked` — réussite, 39 avertissements existants.
- Recette HTTP MCP sur un hôte Windows dédié sans fenêtre, port 18786.
  L’hôte a réussi son test dédié et s’est arrêté normalement.
  Correction de 0,5 px puis restauration du recalage initial, conservation des
  preuves et de l’étalonnage, invalidation de la mesure et de son descendant,
  deux refus des anciens aperçus, nouvelle observation explicitement enregistrée,
  sauvegarde, réouverture et export PDF vérifiés. Les trois mesures natives
  sont conservées. Les anciennes et nouvelles observations figurent au rapport
  avec des identifiants distincts. Les points acceptés restent dans le manifeste.
- PDF de sept pages extrait et première page rendue puis inspectée.
  Poppler signale l’avertissement existant de police `Symbol`.

## Emplacements et rejeu

Tous les chemins ci-dessous sont relatifs au dossier
`O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`.

- Exécutable compilé `target\debug\dper_measure.exe`.
- Test hôte `inspection_mcp_host_tests::owned_headless_mcp_host`, ignoré par
  défaut, démarré avec `DPER_MEASURE_MCP_PORT=18786` et un fichier d’arrêt
  `DPER_TEST_HOST_STOP` propre à la recette. Les répertoires de profil sont
  isolés sous le dossier de sortie.
- Lancement local `output\dpm20-registration-history\run-host.ps1`.
- Client local `output\dpm20-registration-history\recipe.py`, utilisant
  `scripts\smoke_inspection_mcp.py`.
- Entrées synthétiques reprises des appels 7, 9, 11, 13 et 14 de
  `output\dpm20-pr72-mcp-20261114-164052-922\recipe\rpc_evidence_001.json`.
- Résultats `output\dpm20-registration-history\20261115-220723-960`.
  Le dossier contient `rpc_evidence_001.json`, `resultat.json`,
  `historique.dperproj`, le PDF `historique.pdf`, son texte `historique.txt`,
  le manifeste d’export, le rendu `page-1.png` et les journaux de l’hôte.
- Journaux des tests `output\dpm20-registration-history-tests.log`,
  `output\dpm20-registration-history-plan-tests.log`,
  `output\dpm20-registration-history-export-retry.log` et
  `output\dpm20-registration-history-build.log`.

Pour une vérification native, observer puis enregistrer une occurrence d’un
plan recalé, corriger le recalage, consulter l’historique et vérifier que le
suivi demande une nouvelle observation. Restaurer le recalage original ne doit
pas rétablir l’ancienne autorité. Cette recette de fenêtre Windows n’a pas été
exécutée pour cette livraison. Les contrôles exécutés utilisent les fixtures
logicielles et HTTP décrites ci-dessus, avec des images synthétiques.

DPM-20 reste ouvert pour l’orchestration complète. Cette livraison traite la
conservation des preuves lors d’une correction et ne constitue pas une
qualification physique des mesures.
