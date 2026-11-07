# DPM-44 — Redimensionnement numérique

Base : `27e64f1996f9466aea5f0ee9a88064b8f8d7b744` (`dev`, PR #41 fusionnée).

## Comportement livré

En mode Sélection, double-cliquer sur un cartouche ouvre une saisie native avec aperçu.
Une construction sans cartouche s’ouvre par double-clic sur son tracé.

- Segment à deux points : longueur, direction conservée, premier point / point opposé / centre comme ancre.
- Cercle centre/bord : diamètre ou rayon, centre et direction centre–bord conservés.
- Rectangle à deux points ou rectangle orienté à quatre sommets : longueur et largeur, orientation et ancre conservées.
- Autres géométries, rectangles arrondis et polygones non rectangulaires : refus explicite.
- Échelles propres à la géométrie, y compris X/Y ; pixels si aucune calibration complète. Sous échelles anisotropes, la dimension du cercle est directionnelle et ne qualifie pas un diamètre physique unique.
- Point ou virgule décimale. Refus des valeurs non positives, non finies, dégénérées et des géométries sortant de l’image.
- Entrée / Valider : une seule action dans l’historique. Échap / Annuler : aucune modification.
- Capture, révision et objet initial revérifiés avant application ; mutations concurrentes suspendues pendant la saisie.

La saisie impose une géométrie, elle ne détecte aucun contour. L’origine `numeric_resize` et le marqueur de méthode `[dper:numeric_resize:v1]` persistent sans changement de schéma. L’incertitude précédente est retirée, les anciennes preuves et descendants sont invalidés. Un objet imposé ne peut pas servir de source à une nouvelle mesure observée. Les comparaisons et exports ne lui attribuent ni valeur observée, ni écart, ni conformité. La géométrie calculée et sa provenance restent visibles. Les anciennes valeurs en cache des rapports sont également ignorées.

## Vérification logicielle exécutée

| Commande | Résultat |
|---|---|
| `cargo test --locked numeric_resize -- --nocapture` | 19 réussis, 0 échec, 339 filtrés |
| `cargo test --locked recovered_editing_ui_tests::` | 15 réussis, 0 échec, 342 filtrés lors de cette exécution |
| `cargo test --locked measurement_graph::tests::` | 6 réussis, 0 échec, 351 filtrés lors de cette exécution |

40 tests distincts : calculs, calibrations anisotropes, ancres, rectangles orientés/inversés, erreurs, véritables événements egui de double-clic et touches Entrée/Échap, transaction/historique, dépendances, sérialisation/copies, composants MCP, CSV/XLSX et projections de tableaux du rapport.

Le nombre de tests filtrés varie entre les exécutions car deux tests ont été ajoutés pendant la préparation. La dernière exécution du filtre `numeric_resize` couvre les 19 nouveaux tests. Compilation du binaire de tests réussie avec 28 avertissements de code inutilisé. Aucune suite complète, compilation d’installateur, qualification physique ou validation caméra/GPU exécutée pour ce lot. Le contrôle du rapport vérifie les données de tableaux, pas un nouveau rendu visuel PDF. Relecture indépendante statique effectuée, intégration revue par le coordinateur.

## Validation utilisateur

Tester cette branche depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE` avec `cargo run --locked`. L’installateur existant ne contient pas ce changement.

1. Ouvrir une copie d’un projet, puis passer en **Mesures → Sélection**.
2. Créer un segment, double-cliquer sur son cartouche, saisir une longueur avec virgule. Vérifier l’aperçu puis valider. Essayer les trois ancres et vérifier la direction.
3. Sur un cercle centre/bord, essayer diamètre puis rayon : le centre doit rester fixe.
4. Sur un rectangle, modifier les deux dimensions. Sur une construction sans cartouche, double-cliquer sur un bord. Vérifier l’ancre et l’orientation.
5. Essayer zéro, une valeur négative, du texte et une dimension sortant de l’image : aucune application possible. Modifier puis annuler avec Échap.
6. Après une application, faire **Annuler**, puis **Rétablir** : géométrie et provenance doivent suivre une seule action. Les descendants dépendants restent à recalculer.
7. Sauvegarder sous un nouveau nom et rouvrir. Vérifier « Géométrie imposée » et l’absence de conformité attribuée à la valeur saisie.
8. Exporter un rapport et un CSV/XLSX : la provenance doit être explicite, sans valeur observée ni écart de conformité pour l’objet imposé.

La fusion et cette recette restent à la main de l’utilisateur. Le ticket reste en revue jusqu’à acceptation et intégration.
