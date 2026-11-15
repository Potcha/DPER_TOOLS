# DPM-20 — Écarter une observation concurrente avec son motif

Le suivi pouvait signaler plusieurs observations d’un même emplacement sans
proposer de retrait conservant leurs preuves. L’opérateur peut maintenant
sélectionner une observation enregistrée dans **Base documentaire de décision
de conformité**, saisir le **Motif du retrait de l’observation**, puis cliquer
**Écarter cette observation du contrôle**.

L’action conserve les points, profils, réglages, valeur, occurrence, mesure
native et base de décision. Elle ajoute le motif et la révision du retrait,
classe l’observation comme historique et retire son autorité courante.
La mesure concernée et ses descendants deviennent périmés. Les autres branches
du contrôle gardent leurs générations et leurs preuves. Aucune observation
restante n’est approuvée par cette action. Une observation écartée ne peut pas
être réactivée ; une nouvelle observation exige les opérations habituelles.

## Opération commune à l’interface et à MCP

`withdraw_inspection_observation` reçoit `capture_id`, `expected_revision`,
`measurement_id` et `justification`. Après revue explicite de l’opérateur,
utiliser la capture et la révision retournées par `get_inspection_state`.
Le motif doit contenir de 1 à 1 000 caractères après suppression des espaces
extérieurs. La réponse est l’état d’inspection réévalué, avec sa nouvelle
révision et l’avancement du contrôle.

La capture doit être figée et aucune action de tracé ne doit être en cours.
Capture différente, révision périmée, motif vide ou trop long, identifiant
absent, observation non unique, mesure native absente et observation déjà
historique sont refusés avant mutation. Les aperçus n’ont pas d’identifiant
de mesure à retirer. Le retrait rend également les anciens brouillons et
pages d’aperçus périmés par le changement de révision.

Le champ optionnel `withdrawal` conserve le motif et `document_revision` dans
l’observation. Les anciens projets restent lisibles sans migration. La
réouverture conserve le retrait et ne restaure aucune autorité de validation.
Le PDF indique le motif à côté des preuves historiques ; le manifeste conserve
les observations complètes.

## Résultats exécutés le 15 novembre 2026

- 18 tests `inspection_decision_ui::tests::` réussis. Le nouveau scénario
  manipule les contrôles egui par clics et clavier, refuse un motif vide,
  écarte une observation ayant une base de décision revue, conserve ses preuves
  et cette base comme historique, invalide son descendant et préserve une
  mesure indépendante. Sérialisation et note d’export vérifiées.
- 24 tests `inspection_observation_ui::tests::` réussis. Le nouveau scénario
  observe deux occurrences et crée une observation concurrente. Il vérifie les
  refus sans mutation, la résolution de la concurrence, le maintien des deux
  observations restantes et d’une conformité indéterminée. Les refus couvrent
  aussi une identité de mesure réutilisée et une action de calibration active.
- 5 tests `inspection_progress::tests::` réussis. Total de 47 tests ciblés
  distincts, zéro échec. Compilation debug réussie, 39 avertissements existants.
- Hôte Windows HTTP MCP dédié sans fenêtre sur le port 18787, arrêté
  normalement. Cinq refus sans mutation, retrait de la troisième observation
  concurrente, invalidation locale parmi cinq mesures, conservation de
  l’étalonnage, refus d’un second retrait, sauvegarde et réouverture vérifiés.
  Les deux occurrences restent observées avant réouverture ; elles deviennent
  historiques après réouverture selon les règles existantes.
- Export PDF de huit pages et manifeste vérifiés. Texte extrait, page 4
  contenant le motif rendue et inspectée. Avertissement Poppler existant
  concernant la police `Symbol`.

## Emplacements et rejeu

Racine `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`.

- Exécutable `target\debug\dper_measure.exe`.
- Client `output\dpm20-observation-withdrawal\recipe.py` et lancement
  `output\dpm20-observation-withdrawal\run-host.ps1`, utilisant
  `scripts\smoke_inspection_mcp.py` et le test ignoré
  `inspection_mcp_host_tests::owned_headless_mcp_host`.
- Entrées des appels 7, 9, 11, 13 et 14 de
  `output\dpm20-pr72-mcp-20261114-164052-922\recipe\rpc_evidence_001.json`.
- Preuves dans `output\dpm20-observation-withdrawal\20261115-221804-046`.
  Journaux `rpc_evidence_001.json`, `stdout.log`, `stderr.log`, synthèse
  `resultat.json`, projet `retrait.dperproj`, rapport `retrait.pdf`, texte
  `retrait.txt`, manifeste et rendu `page-4.png`.
- Journaux de commandes `output\dpm20-withdrawal-decision-tests.log`,
  `output\dpm20-withdrawal-observation-tests.log`,
  `output\dpm20-withdrawal-progress-tests.log` et
  `output\dpm20-withdrawal-build.log`.

Pour le rejeu manuel, enregistrer deux observations de la même occurrence,
constater le motif de concurrence dans **Avancement du contrôle**, sélectionner
l’observation à écarter, saisir son motif et confirmer le retrait. Vérifier
l’historique, les dépendances et la nécessité de revoir la décision restante.
Sauvegarder puis rouvrir le projet et exporter le PDF pour retrouver le motif.

La fenêtre Windows interactive n’a pas été exercée pour cette livraison.
Les tests utilisent des images synthétiques et ne qualifient pas la mesure
physique. DPM-20 reste ouvert pour l’orchestration complète.
