# DPM-33 / DPM-3 — Socle vectoriel PDF — 31 octobre 2026

Cette tranche est une sous-tâche de DPM-3. Elle livre les bibliothèques de primitives PDF et d'associations explicites, les commandes `drawing-plan`/`drawing-geometry` et leur vérificateur. La reconnaissance sémantique générale et le raccordement MCP/UI restent dans les tickets d'intégration.

Base de la validation : commit `476c4ff2fc11b2e39d2c759ac5fa2bb97e72be9a` (DPM-32, PR #13), lui-même basé sur `dev` `d962946`. Fusionner DPM-32 avant cette tranche. La copie de validation contient uniquement cette base et les fichiers sélectionnés pour DPM-33, dans `output/delivery/DPM-3/`, sans autre worktree ni modification de l'index utilisateur.

## Résultats exécutés

| Contrôle | Résultat |
|---|---|
| `cargo test --locked` | 99 réussis, 0 échec |
| `cargo build --locked` | Réussite |
| Extraction des trois PDF synthétiques livrés | 1, 2 et 2 cercles retrouvés |
| Erreur maximale de centre | 0 point PDF |
| Erreur maximale de rayon | 0,007523681936227433 point PDF, seuil 0,15 |
| Sources PDF et empreintes du manifeste | Identiques avant/après extraction ; toutes conformes |
| Texte/glyphes exclus des primitives | Vérifié sur les trois PDF |
| Confirmation automatique de primitives | Aucune |
| `drawing-plan` sur le premier PDF | Diamètre nominal 4,000 mm, limites 3,900/4,100 mm, non confirmé |
| Six appels CLI réels | Deux réussites ; écrasement, pages 0/21 et source absente refusés |

Les douze tests des modules vectoriel/association couvrent notamment transformations héritées, clipping et peintures non résolus, sous-chemins, cercles complets, alternatives concentriques/symétriques, justification, import JSON sans revue, changement des sources et incohérences de vues/identifiants. La régression CLI supplémentaire vérifie que les deux commandes atteignent leur adaptateur plutôt que l'aide d'une commande inconnue, sans créer de sortie pour une source absente.

Le compte rendu machine portable [evidence/dpm33.json](evidence/dpm33.json) contient les empreintes du code évalué, les comparaisons cercle par cercle et les résultats des contrôles. Les journaux bruts restent dans `output/delivery/DPM-3/tests.log`, `build.log` et dans les sous-dossiers `dpm33-vector-final`/`dpm33-cli-final` de la copie de validation.

## Reproductibilité des fixtures

Lors de l'export de la copie de validation avec la configuration Git locale, les PDF synthétiques ont subi une conversion LF vers CRLF : leurs octets ne correspondaient plus aux empreintes et aux offsets produits par le générateur. Les PDF régénérés sont identiques aux blobs Git originaux ; aucune modification de ces blobs, des valeurs attendues ou du manifeste n'est nécessaire. Une règle `.gitattributes` désactive désormais la conversion de fins de ligne pour ces seuls PDF. Le vérificateur utilise `tests/fixtures/drawing_geometry/` par défaut, au lieu d'une sortie locale antérieure.

Pour reproduire depuis `DPER_MEASURE/`, avec Poppler accessible dans le PATH :

```powershell
cargo test --locked
cargo build --locked
python scripts/verify_vector_fixtures.py --output-dir output/automation-validation/dpm33-nouvel-essai
```

Les avertissements existants de code inutilisé et de copie du cache incrémental restent présents. Aucun plan industriel, projet de pièce ou résultat physique n'est inclus. Ces seuils contrôlent l'extraction géométrique synthétique ; ils ne qualifient ni un microscope, ni l'incertitude d'une mesure réelle, ni une conformité physique. Les vues/associations restent explicites et la proximité ne prouve pas la sémantique d'une cote. Le parent DPM-3 demeure en cours.
