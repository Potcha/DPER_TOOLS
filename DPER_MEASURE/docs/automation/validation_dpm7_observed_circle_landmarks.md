# DPM-7 — appariement de centres observés sur la photo

`propose_circle_landmark_matches` compare de trois à six cercles d’une vue PDF courante à des cercles recherchés sur la capture figée. Le client fournit les identifiants du catalogue PDF et des zones approximatives en pixels natifs. L’application ajuste les cercles sur les contours, puis associe chaque cercle PDF à un centre photo distinct. Le nombre de recherches photo peut dépasser celui du plan, avec un maximum de huit recherches et de 720 affectations. Un dépassement est refusé avant les ajustements. La réponse conserve les preuves PDF et photo, les résidus des hypothèses retenues et les raisons des rejets.

## Validation dans DPER_MEASURE

1. Enregistrer le travail en cours, fermer l’ancienne instance et lancer dans PowerShell :

   ```powershell
   & 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm7-circle-pool-app\debug\dper_measure.exe'
   ```

2. Ouvrir `photo-reperes.png` via **Fichiers → Image**, puis importer `reperes-circulaires.pdf` comme plan de référence. Les deux fichiers sont dans `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm7-circle-pool-mcp-20261110-221204-874\recipe`.
3. Demander dans le chat **« Exécute la recette DPM-7 des repères circulaires avec distracteurs via MCP »**. Le chat prépare les vues PDF de test, puis compare l’état de la session avant et après chaque diagnostic. Le résultat apparaît dans la réponse MCP.
4. Vérifier les résultats attendus :

   | Scénario | Résultat |
   | --- | --- |
   | Triangle asymétrique, recherches fournies en ordre inverse | Une hypothèse sur six. Les centres ajustés sont à moins de 0,3 px par axe des centres synthétiques et diffèrent des amorces. |
   | Carré | Quatre hypothèses sur 24. L’état est `ambiguous`. |
   | Triangle et un cercle d’un autre motif sur la photo | Six permutations rejetées. L’état est `no_match`. |
   | Triangle complet et un cercle supplémentaire | Une hypothèse parmi 24. Le quatrième repère reste consultable avec ses preuves de contour. |
   | Deux triangles translatés avec la même géométrie | Deux hypothèses parmi 120. L’état est `ambiguous` et aucun identifiant unique n’est proposé. |
   | Quatre cercles PDF et sept recherches photo | Refus explicite des 840 affectations, au-delà du maximum de 720. |
   | Triangle complet et zone supplémentaire sans contour | Refus identifiant la recherche en échec. |
   | Révision périmée, capture erronée, cercle PDF hors vue, recherches superposées ou contour manquant | Refus. |

## Reproduction et limites

Depuis le dossier DPER_MEASURE, `python scripts/verify_observed_circle_landmarks_mcp.py output/dpm7-observed-fixtures` régénère les deux entrées dans un dossier neuf. L’option `--test-port` lance le scénario intégral sur un hôte dédié, y compris le remplacement temporaire du PDF synthétique et sa restauration. Les requêtes résolues par l’hôte de test sont dans `summary.json` et le journal RPC local. Pour l’instance utilisateur, retrouver les identifiants de vues, la capture et la révision de la session courante. Le cas de remplacement du PDF est réservé au dossier de test.

Les centres proviennent des pixels sous les zones indiquées et tous les cercles fournis doivent être ajustables dans leurs disques de recherche disjoints. La comparaison se limite aux similarités préservant l’orientation et utilise tous les cercles PDF dans chaque hypothèse. Les rayons décrivent les contours observés. Chaque résultat reste une hypothèse à examiner. Le recalage de la session et l’étalonnage passent par leurs opérations propres. La recherche globale de repères sur la photo reste à développer.

## Résultats exécutés

### Repères supplémentaires et motifs concurrents

- `cargo test --locked --bin dper_measure landmark_matching::` — 16 réussis, 0 échec, 436 filtrés, dont deux nouveaux tests. Compilation des tests avec 30 avertissements de code inutilisé.
- Recette sur l’exécutable Windows de test via MCP dédié, port 18773 — 13 contrôles réussis, dont quatre nouveaux. Extraction PDF par Poppler, état comparé après chaque diagnostic et fermeture normale de l’application. Preuves dans `output/dpm7-circle-pool-mcp-20261110-221204-874/recipe/summary.json`.
- Compilation debug réussie avec 40 avertissements de code inutilisé. Les entrées sont synthétiques et le contrôle porte sur les réponses MCP.

### Première livraison avec ensembles de même taille

- `cargo test --locked landmark_matching::` a réussi 14 tests distincts dont cinq nouveaux, avec 430 tests filtrés. Le cas du cercle ajusté débordant de son disque, détecté en revue, est couvert.
- L’hôte MCP dédié a exécuté neuf scénarios avec extraction PDF réelle. Les trois diagnostics et les six refus attendus ont réussi. La comparaison d’état a couvert le changement externe du PDF et sa restauration. L’hôte s’est arrêté normalement.
- La compilation debug a réussi avec 40 avertissements de code inutilisé. Les journaux locaux sont `output/dpm7-observed-tests.log` et `output/dpm7-observed-build.log`. Les preuves MCP sont sous `output/dpm7-observed-mcp-20261110-133633/recipe`.
