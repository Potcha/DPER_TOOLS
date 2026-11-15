# DPM-20 — Revoir et enregistrer un aperçu groupé

## Recette Windows

1. Enregistrer le projet ouvert et fermer l’ancienne instance. Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, compiler avec `cargo build --locked` sur la branche de la PR ou sur `dev` après fusion. Lancer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`.
2. Ouvrir une copie d’un projet préparé avec photo figée, plan courant, cotes circulaires associées et recalage prêt. Pour tester plusieurs occurrences, choisir une cote répétée avec au moins deux contours manquants. Les tests automatisés construisent leurs propres données synthétiques.
3. Dans **Plan de contrôle et inspection IA**, cliquer **Recommencer les aperçus (8 maximum)**. Vérifier que le nombre de mesures reste identique.
4. Dans **Avancement du contrôle**, cliquer **Examiner l’aperçu** sur la première occurrence de la page, même si une autre a été calculée après. Ouvrir **Observer les contours de la photo**. Le contour et ses preuves doivent être disponibles dans le brouillon.
5. Examiner le contour et cliquer **Enregistrer explicitement l’observation**. Vérifier qu’une seule mesure est ajoutée, avec son groupe et sa preuve. Pour la suivante, relancer les aperçus sur la nouvelle révision.
6. Refaire l’essai en modifiant un réglage de recherche dans le brouillon. Vérifier que l’enregistrement exige un nouvel aperçu compatible.

Le parcours réutilise les aperçus de la dernière page. Un enregistrement change la révision. La décision de conformité continue de dépendre des preuves et de la qualification applicables.

## Même parcours via MCP

Sur une instance dédiée et le même projet de recette, relever la capture et la révision via `get_inspection_state`, puis appeler `preview_control_plan` avec `limit: 1`. Examiner le résultat du premier travail marqué `previewed`. Après revue explicite, appeler `record_control_plan_preview` avec les champs suivants.

| Champ | Valeur |
| --- | --- |
| `capture_id` | Identifiant de capture renvoyé par la page |
| `expected_revision` | `document_revision` renvoyé par la page |
| `feature_id` | Identifiant du travail examiné |
| `occurrence_id` | Identifiant de l’occurrence examinée, ou `null` pour une cote simple |

L’enregistrement doit produire une seule mesure et conserver le groupe, l’occurrence et les preuves du contour. Un travail encore en attente, un brouillon natif ouvert ou un contexte modifié doit entraîner un refus avant enregistrement. Répéter l’appel avec la révision précédente doit échouer. Relancer ensuite `preview_control_plan` sans curseur avec la nouvelle révision et vérifier que la recherche passe à l’occurrence manquante. Terminer avec le rapport PDF habituel et contrôler que la mesure enregistrée y figure et que l’occurrence encore à observer apparaît dans la couverture.

## Vérification logicielle

Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, lancer `cargo test --locked inspection_observation_ui::tests::`. Les cas ajoutés couvrent la reprise native de la première occurrence, l’enregistrement explicite et les refus après changement du groupe, du catalogue ou des pixels. Les tests existants couvrent aussi la pagination, les brouillons et la reprise sur occurrence manquante.

## Résultats du raccordement MCP

Sur la branche `codex/DPM-20-record-preview`, après intégration de `dev`
`12aff7c`, 22 tests d’observation réussissent. La compilation debug réussit
avec 39 avertissements de code inutilisé existants.

Le rejeu HTTP sur Windows utilise un hôte dédié au port 18783. Il ouvre
le projet synthétique de la recette précédente, analyse le PDF, définit
la vue, affecte les deux occurrences puis établit le recalage. Il vérifie
la page d’un aperçu, le refus de l’occurrence en attente, l’enregistrement
de la première occurrence, le refus d’un doublon avec l’ancienne révision,
la reprise sur la deuxième occurrence et la réouverture du projet sauvegardé.
Une seule mesure est créée, de diamètre 3,507436 mm.

Les chemins du rejeu sont les suivants.

- Projet source `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-pr72-mcp-20261114-164052-922\recipe\occurrence-checks\synthetic_reviewed.dperproj`
- Journal `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-record-preview\20261115-204419-391\rpc_evidence_001.json`
- Projet sauvegardé `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-record-preview\20261115-204419-391\controle.dperproj`
- Rapport `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-record-preview\20261115-204419-391\controle.pdf`
- Texte extrait `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-record-preview\20261115-204419-391\controle.txt`

Le PDF conserve les preuves et indique deux occurrences attendues,
une observation enregistrée et deux aperçus. La dernière page a été rendue
et inspectée visuellement. Poppler signale la police Symbol, avec un diamètre
lisible dans la page rendue. L’hôte s’est arrêté normalement.

Les tests egui couvrent le parcours natif. La fenêtre Windows interactive
reste à vérifier sur cette livraison. DPM-20 reste ouvert pour
l’orchestration complète et les critères regroupés de revue des hypothèses.
