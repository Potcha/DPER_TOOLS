# DPM-3 — Vérifier les associations documentaires du PDF

Lancer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm3-support-associations-app\debug\dper_measure.exe` après avoir enregistré tout projet ouvert et fermé l'ancienne instance. Les essais utilisent des PDF synthétiques. Aucune photo ni calibration n'est requise pour les trois premiers contrôles.

## Recette dans l'application

Pour chaque PDF, utiliser **Inspection → Importer un plan de référence**, puis **Plan de référence → Géométries proposées du PDF → Page 1 → Analyser / réanalyser la page**. Poursuivre dans **Vues et associations du plan PDF**.

1. Ouvrir `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\tests\fixtures\drawing_geometry\03_two_symmetric_views.pdf`. Développer **Indices de vues et de rôles des traits** et cliquer sur **Rechercher les indices dans la page**. Attendre **VUE A** et **VUE B**, chacun avec un contour candidat. Développer les sources pour vérifier les boîtes, les identifiants des mots et la provenance vectorielle. La vue reste à choisir.
2. Ouvrir `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm3-support-associations-manual\linear\supports-lineaires.pdf`. Préparer une vue, sélectionner la cote **18 mm** et les deux traits `(40, 80)–(160, 80)` et `(60, 120)–(140, 120)`, puis enregistrer la vue explicite. Choisir la vue et la cote dans **2. Choisir une association**, préparer le diagnostic, choisir les deux traits et expliquer le choix. Attendre **40,00 pt PDF** de séparation et **80,00 pt PDF** de recouvrement. Cliquer sur **Enregistrer l'association documentaire**, puis ouvrir **Supports enregistrés** et vérifier que les sources sont consultables.
3. Ouvrir `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm3-support-associations-manual\angular\supports-angulaires.pdf`. Refaire la procédure avec la cote **123°** et les traits `(60, 100)–(180, 100)` et `(80, 60)–(160, 140)`. Attendre **45° et 135°** à l'intersection `(120, 100)` pt. L'enregistrement est désactivé jusqu'au choix d'un secteur. Choisir l'une des propositions à **45°** et vérifier sa valeur et ses deux extrémités dans **Supports enregistrés**. Le nominal reste à **123°**.
4. Pour contrôler la persistance dans un fichier projet, ouvrir une image d'essai avant l'import du plan angulaire, refaire l'étape 3, puis enregistrer et rouvrir le projet. La preuve et le secteur doivent rester consultables avec l'état **Association suspendue**. Réanalyser la source et refaire le choix pour rétablir une association courante. Vérifier qu'aucune mesure photo n'a été créée.

Pour régénérer les deux PDF dans des dossiers neufs, exécuter depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE` les commandes suivantes, puis ouvrir les fichiers aux chemins imprimés.

```powershell
python scripts/verify_linear_drawing_supports_mcp.py output/dpm3-recipe-linear
python scripts/verify_angular_drawing_supports_mcp.py output/dpm3-recipe-angular
```

## Résultats automatisés

- `cargo test --locked --bin dper_measure support` donne **51 réussites**, 409 filtrés.
- `cargo test --locked --bin dper_measure drawing_scope_hypotheses::` donne **3 réussites**, 457 filtrés.
- `cargo test --locked --bin dper_measure session_schema_versions_are_supported` donne **1 réussite**, 459 filtrés.
- `cargo test --locked --bin dper_measure inspection_plan_ui::tests::` donne **19 réussites**, 441 filtrés. Le premier essai restreint avait échoué lors de l'installation d'un PDF temporaire avec l'erreur Windows 650. La relance hors de cet environnement réussit.
- Au total, **67 tests Rust distincts réussis** dans ces filtres. La compilation debug réussit avec 39 avertissements existants de code inutilisé.
- Transport MCP réel sur hôte dédié 18771 et extraction Poppler. Le script `scripts/verify_drawing_support_associations_mcp.py` vérifie deux vues à confirmer, les deux associations enregistrées, les refus sur révision périmée, secteur absent et vues concurrentes, et la suspension des deux associations touchées. L'hôte s'arrête normalement. Les journaux RPC sont dans `output/dpm3-support-associations-mcp-20261111-102543/recipe/`.

L'outil MCP `inspect_drawing_scope_hypotheses` propose des périmètres à partir de libellés explicites localisés et de contours rectangulaires voisins. Les traits discontinus donnent un indice de rôle auxiliaire. Les autres rôles et les cas ambigus restent à interpréter. Le choix fait dans l'interface ou via `select_drawing_supports` consigne une interprétation documentaire. Toute comparaison physique exige ensuite une mesure sur la photo.
