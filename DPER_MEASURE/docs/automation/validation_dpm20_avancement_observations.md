# DPM-20 — Avancement fondé sur les observations enregistrées

Les motifs des aperçus étaient ajoutés aux blocages du contrôle même lorsqu’une
observation native courante et validée existait déjà pour la même cible. Une
décision revue pouvait ainsi proposer le rapport tout en affichant « mesure
native absente ou aperçu non enregistré », « montage de mesure non qualifié »
et « incertitude totale non renseignée » provenant d’un simple aperçu.

Le calcul commun à **Avancement du contrôle** et à `get_inspection_state`
ignore désormais les blocages d’un aperçu lorsqu’une observation enregistrée
et validée couvre la même caractéristique, la même capture et la même
occurrence. Les aperçus et toutes leurs preuves restent consultables.
Les motifs des observations enregistrées restent appliqués. Les limites des
aperçus d’autres occurrences encore manquantes restent visibles.

Une observation historique, écartée ou périmée ne masque pas ces limites.
Le retrait d’une observation ramène le suivi vers l’occurrence à observer.
Le changement ne crée aucune mesure et n’accorde aucune nouvelle conformité.

## Résultats exécutés le 15 novembre 2026

- Régression reproduite avant correction dans
  `progress_ignores_preview_blockers_replaced_by_a_current_recorded_observation`.
  Trois motifs d’aperçu étaient affichés alors que la décision enregistrée,
  revue par les contrôles natifs egui, était conforme dans la fixture.
- 19 tests `inspection_decision_ui::tests::`, 25 tests
  `inspection_observation_ui::tests::` et 5 tests
  `inspection_progress::tests::` réussis. Total de 49 tests distincts.
  Les deux nouveaux tests vérifient la conservation des preuves, la réponse MCP,
  le suivi par occurrence, les limitations des cibles manquantes, la lecture
  sans mutation et le retour aux blocages après retrait ou péremption.
- Recette HTTP MCP réelle sur hôte Windows dédié sans fenêtre, port 18788,
  arrêt normal. Deux occurrences synthétiques observées successivement.
  L’aperçu non enregistré de la deuxième occurrence conserve son motif tant
  qu’elle manque. Après son enregistrement, le motif disparaît du suivi.
  Un aperçu supplémentaire de la première occurrence laisse l’avancement et
  les mesures inchangés. Le retrait de sa mesure rétablit le motif et désigne
  cette occurrence comme prochaine observation. La conformité reste indéterminée
  faute de base physique revue dans cette recette HTTP.
- Compilation debug réussie avec 39 avertissements existants.

## Emplacements et rejeu

Racine `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`.

- Exécutable `target\debug\dper_measure.exe`.
- Client `output\dpm20-progress-observations\recipe.py` et lanceur dédié
  `output\dpm20-progress-observations\run-host.ps1`, utilisant
  `scripts\smoke_inspection_mcp.py` et le test ignoré
  `inspection_mcp_host_tests::owned_headless_mcp_host`.
- Entrées des appels 7, 9, 11, 13 et 14 de
  `output\dpm20-pr72-mcp-20261114-164052-922\recipe\rpc_evidence_001.json`.
- Résultats `output\dpm20-progress-observations\20261115-222747-842`.
  Journaux `rpc_evidence_001.json`, `stdout.log`, `stderr.log` et synthèse
  `resultat.json`.
- Journaux de commandes `output\dpm20-progress-regression-before.log`,
  `output\dpm20-progress-tests.log`, `output\dpm20-progress-decision-tests.log`,
  `output\dpm20-progress-observation-tests.log`,
  `output\dpm20-progress-summary-tests.log` et `output\dpm20-progress-build.log`.

Pour le rejeu manuel, préparer puis enregistrer une occurrence. Dans
**Avancement du contrôle**, son ancien aperçu ne doit plus ajouter le motif
d’absence de mesure. Prévisualiser une deuxième occurrence sans l’enregistrer
doit laisser ce motif disponible pour cette cible. Enregistrer la deuxième
observation puis écarter la première avec justification doit faire réapparaître
la première occurrence comme prochaine observation à réaliser.

La fenêtre Windows interactive et un nouvel export PDF n’ont pas été exercés
pour cette correction du suivi. Les tests sont synthétiques et ne qualifient
pas les mesures physiques. DPM-20 reste ouvert pour l’orchestration complète.
