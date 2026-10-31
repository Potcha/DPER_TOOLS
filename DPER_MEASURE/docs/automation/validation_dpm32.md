# DPM-32 — Schémas MCP des points — 31 octobre 2026

Base vérifiée : `dev` au commit `d962946bbbf43281c2460fb12bee878033667617`.

Une copie exacte de cette base avec les prochains modules DPM-3 a révélé six erreurs de compilation E0277 : les types publics `Correspondence` et `InspectionGeometry` dérivent `JsonSchema`, mais leur type commun `geometry::Point` n'avait pas livré ce trait. L'arbre local contenait déjà ce raccordement ; les essais précédents de cet arbre n'avaient pas vérifié son inclusion dans les PR.

Le correctif ajoute seulement l'import `rmcp::schemars` et le derive de `Point`. Un test de régression génère les schémas des deux entrées publiques et vérifie leurs coordonnées x/y numériques obligatoires. Aucun autre changement local de `geometry.rs` n'est inclus.

## Contrôles exécutés sur le contenu exact de la PR

- `cargo test --locked` : **86 réussis, 0 échec**, y compris `public_inspection_inputs_include_numeric_point_schemas`.
- `cargo build --locked` : réussite.

La validation utilise une copie temporaire dans `output/delivery/DPM-32/`, dans le même dossier DPER_MEASURE, sans créer de worktree ni changer la branche ou l'index de travail. Les sources sont celles de la base indiquée plus le correctif et le test. Les journaux locaux sont `output/delivery/DPM-32/tests.log` et `build.log`.

Deux anciens tests de rapports écrivent dans `CARGO_MANIFEST_DIR/target` : ce répertoire a été créé dans la copie de validation après leur premier échec de préparation de dossier, puis la suite complète a été rejouée. Les avertissements de code inutilisé et de copie du cache incrémental restent présents.

Cette vérification établit la compilation et les contrats JSON des composants livrés ; elle ne constitue pas une recette UI/MCP de bout en bout ni une qualification physique.
