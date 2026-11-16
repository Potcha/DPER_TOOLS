# DPM-17 — Recette consolidée des opérations d’inspection

La recette a été exécutée sur une copie du commit `374f201` de `dev`, en
combinant la suite Rust et un rejeu HTTP MCP sur un hôte Windows dédié. Les
données de test sont synthétiques. Les opérations MCP utilisent l’application
pour lire la capture, contrôler le contexte et produire les preuves.

| Périmètre | Vérification sur cette version | Résultat |
| --- | --- | --- |
| DPM-18, contexte et mesure | Envoyer une capture incorrecte, une révision périmée et des prétentions de validation côté client. Comparer le diamètre enregistré aux pixels et au nominal du plan. | Les erreurs de contexte préservent la révision et les mesures. L’autorité reste dans l’application. Le diamètre relevé de 4,207240 mm se distingue du nominal de 4,000 mm. |
| DPM-19, dépendances et conservation | Modifier le PDF, exercer les suppressions et undo/redo, puis rouvrir les projets valides et incompatibles. Exporter le rapport des dépendances. | Les descendants sont à recalculer et la mesure indépendante conserve sa valeur. Annuler une édition ne réactive pas l’ancienne preuve. Les projets de schéma futur ou cycliques sont refusés sans changer le contexte. |
| DPM-20, du plan au rapport | Exécuter le test composé natif et son rejeu indépendant via l’adaptateur MCP. Dans la recette HTTP, enregistrer deux occurrences aux valeurs distinctes, vérifier leurs preuves, puis rejouer le changement d’affectation et la réouverture. | La sauvegarde, la réouverture et les exports du test composé sont passés. Le rapport HTTP conserve les preuves des deux occurrences. Un changement d’affectation rend périmées les observations concernées et leur descendant. La moyenne au nominal n’accorde pas de conformité au groupe. |

Les **531 tests Rust** ont réussi. L’unique test ignoré par défaut a ensuite
fourni l’hôte HTTP et s’est arrêté normalement. Le journal du scénario contient
**249 appels répartis sur 21 opérations**, dont sept cas défavorables de vision
dans la recette. Le PDF des dépendances compte cinq pages. La page 5 rendue
montre deux lignes « À recalculer » et la valeur indépendante de 1,050 mm.
Poppler signale une police Symbol indisponible. La page reste lisible.

Le test composé exerce les contrôles natifs dans la suite Rust. La recette
en fenêtre Windows se trouve dans le compte rendu antérieur
[du plan au rapport](validation_dpm20_parcours_complet.md). Elle n’a pas été
rejouée pour cette consolidation. Les limites des contours et les essais
physiques restent documentés dans [DPM-12](validation_dpm12_photos_reelles.md)
et le [protocole de qualification](protocole_qualification_physique_dpm26.md).

## Rejeu et preuves

- Suite Rust complète avec `cargo test --locked`.
- Recette HTTP avec `scripts/smoke_inspection_mcp.py`, mode `fixtures` puis
  `run` vers un hôte de test isolé. La sortie locale
  `output/dpm17-convergence` contient le lanceur `run.ps1` et les journaux.
- Le dossier `baseline/rpc` contient `smoke_summary.json`,
  `rpc_evidence_001.json`, les projets, PDF et manifestes générés.
  Le rendu contrôlé est `dependency-5.png` dans le dossier de sortie.
