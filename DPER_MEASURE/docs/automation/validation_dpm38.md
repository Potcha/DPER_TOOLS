# DPM-38 — Intégrité du périmètre des vues PDF

Base : `e0e45547039117858ece300fc721ebaf331ff00c`. DPM-38 est une sous-tâche de DPM-3, qui reste ouvert. **Statut : `software_validation_passed`** pour l'intégrité de source des vues et ses chemins d'utilisation. Ce statut décrit les contrôles logiciels consignés ci-dessous ; il ne vaut ni qualification physique ni validation visuelle de l'interface.

## Contrat livré

`DrawingView.source_proof` est une preuve source optionnelle et versionnée. Son empreinte canonique couvre le contexte d'extraction et le contenu de tous les membres sélectionnés, y compris un changement de contenu qui conserve la boîte englobante. Elle exclut les états de revue/mesure et les membres hors périmètre. Les vues dupliquées ou ambiguës sont refusées par la validation et la recherche de vue unique.

Une vue sans preuve reste historique et suspendue. La validation s'effectue à chaque utilisation avec le catalogue courant ; le JSON sérialisé ne suffit pas à établir l'autorité de la source. Les catalogues sont des caches runtime non restaurés depuis le projet. L'évaluation emprunte les catalogues, sans copie du cache par image. Le scope de recalage conserve le `view_id` : si la preuve change l'identité, le recalage est périmé. Après reprise d'un projet issu d'une version antérieure, il faut redéfinir explicitement la vue puis établir un nouveau recalage. Aucune revalidation silencieuse n'est effectuée et aucun schéma 17 n'est prévu.

Les changements de dépendance invalident aussi les descendants liés aux vues, y compris les enregistrements avec scope non lié à une caractéristique. Les générations d'import ne doivent pas permettre une collision d'identité. Une mesure manuelle conservée n'est pas supprimée par cette invalidation.

## Résultats des contrôles

Les journaux finaux se trouvent dans `output/delivery/DPM-3-view-scope-integrity/checks/`. Chaque filtre ciblé nommé ci-dessous a terminé avec au moins un test réussi et zéro échec.

| Contrôle ciblé | Résultat |
|---|---:|
| `dper_binding` — `dpm38-model-02.log` | 41 réussis, 0 échec, 0 ignoré |
| `view_scope` — `dpm38-assessment-03.log` | 11 réussis, 0 échec, 0 ignoré |
| acceptation restauration — `dpm38-restore-02.log` | 1 réussi, 0 échec, 0 ignoré |
| `registration_ui` — `dpm38-registration-ui.log` | 17 réussis, 0 échec, 0 ignoré |
| décision revue — `dpm38-decision.log` | 1 réussi, 0 échec, 0 ignoré |

La recette complète finale est `snapshot/DPER_MEASURE/output/automation-validation/dpm38-run-02/` : **276 tests réussis, 0 échec, 1 ignoré** ; le contrôle explicite de l'hôte headless a réussi (1/1). La parité native/MCP est vraie, l'isolation est terminée et le processus de test a été arrêté. Le smoke MCP a couvert 18 outils et 7 cas adverses. Les acceptations synthétiques antérieures restent conformes : 17 cas de pointes triangulaires, 19 cas d'attaches et 11 cas d'unités.

Le journal de compilation du binaire applicatif signale **21 avertissements** : 17 préexistants et 4 nouveaux avertissements d'enveloppes de compatibilité inutilisées dans ce binaire (`attach_decision_evidence`, `inspect_feature`, `assess_record`, `assess_record_with_fingerprint`), qui restent utilisées par les tests. Le binaire de tests signale 12 avertissements. Ces nombres sont ceux du run-02 ; les 4 avertissements ne sont pas comptés parmi les avertissements préexistants.

## Incidents intermédiaires consignés

Les journaux d'échec ont été conservés séparément et ne sont pas inclus dans les compteurs finaux :

- Le premier filtre modèle a eu 39 réussites et 2 échecs : deux anciens fixtures se sont retrouvés suspendus après introduction de l'identité de preuve source. Le filtre modèle final a ensuite réussi (41/41).
- Le premier test de restauration a échoué : le simple rendu faisait passer la révision de 2 à 3. Une génération historique pouvait coïncider avec la génération réinitialisée à l'import. Le garde d'invalidation exclut désormais les observations importées, déjà suspendues ; les descendants importés sont déjà périmés au chargement. Les assertions supplémentaires vérifient la conservation de `source_proof`, l'absence des catalogues runtime et l'état courant faux après rechargement. Le test final a réussi sans retirer l'exigence de lecture seule.
- La première recette complète (`dpm38-run-01`) a eu 275 réussites, 1 échec et 1 ignoré : une décision revue restait attendue comme `Validated` après modification de bornes de vue. L'attente du test a été corrigée pour vérifier la suspension et les motifs `drawing_binding_changed` et `registration_missing_or_stale` ; run-02 a réussi.
- La revue indépendante Sol a identifié trois chemins de catalogue qui ne transmettaient pas la source courante : inspection directe, attestation de décision et scope de recalage dans une inspection non liée. Leur propagation a été corrigée. Des régressions ont également couvert les collisions de génération à l'import et l'invalidation des descendants pour un scope non lié, tout en préservant les mesures manuelles. Les ciblés finaux correspondants ont réussi.

Les journaux intermédiaires restent dans le dossier `checks/` et dans `dpm38-run-01`. Ils constituent l'historique des corrections, pas des échecs du run final. La revue indépendante citée portait sur les trois chemins de propagation ; elle ne constitue pas une revue finale indépendante de toutes les assertions ou guards.

## Portée et limites

La recette vérifie le comportement logiciel avec des cas synthétiques, les sauvegardes/restaurations et les échanges natifs/MCP. Elle ne comprend pas de QA visuelle de l'interface, de contrôle GPU, de contrôle visuel de rapports, ni de qualification physique. Elle ne démontre pas la reconnaissance automatique d'une vue ou l'interprétation sémantique d'un plan quelconque. Aucune calibration physique, mesure de pièce ni conformité n'est revendiquée. DPM-3 reste ouvert.

Routage de cette étape : moteur et revue des chemins par GPT-6.1 Sol, effort Medium ; documentation et exécution ciblée par GPT-6 Luna, effort Medium ; intégration et coordination par le coordinateur.
