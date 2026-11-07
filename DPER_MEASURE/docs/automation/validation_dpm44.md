# DPM-44 — Redimensionnement numérique

Base : `27e64f1996f9466aea5f0ee9a88064b8f8d7b744` (`dev`, PR #41 fusionnée).

## Comportement livré

En mode Sélection, double-cliquer sur un cartouche active une saisie native directement dans le cartouche, avec aperçu, sans fenêtre ni fond assombri.
Une construction sans cartouche s’ouvre par double-clic sur son tracé.

- Segment à deux points : longueur, direction conservée, premier point conservé.
- Cercle centre/bord : diamètre affiché dans le cartouche, centre et direction centre–bord conservés.
- Rectangle à deux points ou rectangle orienté à quatre sommets : longueur et largeur, orientation et premier point conservés.
- Autres géométries, rectangles arrondis et polygones non rectangulaires : refus explicite.
- Échelles propres à la géométrie, y compris X/Y ; pixels si aucune calibration complète. Sous échelles anisotropes, la dimension du cercle est directionnelle et ne qualifie pas un diamètre physique unique.
- Point ou virgule décimale. Refus des valeurs non positives, non finies, dégénérées et des géométries sortant de l’image.
- Entrée : une seule action dans l’historique. Échap ou clic ailleurs : annulation sans modification.
- Capture, révision et objet initial revérifiés avant application ; mutations concurrentes suspendues pendant la saisie.

La saisie impose une géométrie, elle ne détecte aucun contour. L’origine `numeric_resize` et le marqueur de méthode `[dper:numeric_resize:v1]` persistent sans changement de schéma. L’incertitude précédente est retirée, les anciennes preuves et descendants sont invalidés. Un objet imposé ne peut pas servir de source à une nouvelle mesure observée. Les comparaisons et exports ne lui attribuent ni valeur observée, ni écart, ni conformité. La géométrie calculée et sa provenance restent visibles. Les anciennes valeurs en cache des rapports sont également ignorées.

## Vérification logicielle initiale

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
2. Créer un segment, double-cliquer sur son cartouche, saisir une longueur avec virgule directement à la place de la valeur. Vérifier l’aperçu puis appuyer sur Entrée. Vérifier la direction et la première extrémité fixe.
3. Sur un cercle centre/bord, changer le diamètre dans le cartouche : le centre doit rester fixe.
4. Sur un rectangle, modifier les deux dimensions. Sur une construction sans cartouche, double-cliquer sur un bord. Vérifier le premier point fixe et l’orientation.
5. Essayer zéro, une valeur négative, du texte et une dimension sortant de l’image : aucune application possible. Modifier puis annuler avec Échap.
6. Après une application, faire **Annuler**, puis **Rétablir** : géométrie et provenance doivent suivre une seule action. Les descendants dépendants restent à recalculer.
7. Sauvegarder sous un nouveau nom et rouvrir. Vérifier que le cartouche conserve uniquement le nom et la valeur ; la traçabilité reste dans les détails et exports, sans conformité attribuée à la saisie.
8. Exporter un rapport et un CSV/XLSX : la provenance doit être explicite, sans valeur observée ni écart de conformité pour l’objet imposé.

La fusion et cette recette restent à la main de l’utilisateur. Le ticket reste en revue jusqu’à acceptation et intégration.

## Ajustement ergonomique demandé le 8 novembre 2026

La fenêtre a été remplacée par des champs dans le cadre existant. Le cartouche affiche uniquement le nom (sans doublon M7 · M7), la valeur et l’unité ; le préfixe « Géométrie imposée » en est retiré. La provenance interne, les détails, les exports et les protections de conformité sont conservés. Le diamètre est édité pour le cercle, les deux dimensions pour le rectangle. La première extrémité / le premier coin reste fixe ; le centre du cercle reste fixe. Le premier champ est sélectionné à l’ouverture. Entrée valide ; Échap ou clic ailleurs annule. L’aperçu remplace le tracé initial au lieu de superposer deux cartouches. Les valeurs préremplies sont lisibles, sans bruit du type 0,9999999999999999 ; valider un champ inchangé ne modifie pas la géométrie exacte.

Vérification ciblée de cette révision : filtre `numeric_resize` (21 tests) et `recovered_editing_ui_tests::` (15 tests), soit 36 tests distincts réussis, zéro échec. Deux nouveaux tests contrôlent la frappe réelle dans le cadre, les textes effectivement rendus, le clic extérieur et la précision inchangée. Pas de nouvelle suite complète ni d’installateur pour cette révision.

### Correctif de lisibilité du champ

Le champ héritait du fond clair global alors que son texte était blanc. Les deux champs numériques utilisent maintenant explicitement un fond sombre, avec sélection bleue et texte blanc. `cargo test --locked numeric_resize_ui::tests::` : 9 réussis, zéro échec, 352 filtrés. Le nouveau test contrôle les valeurs préremplies et les couleurs réellement peintes en thèmes clair/sombre, avec et sans focus, après l’animation d’ouverture. Les premiers essais du nouveau test comparaient les couleurs pendant cette animation ; l’attente de stabilisation a été ajoutée avant les assertions. Aucun autre test relancé pour ce correctif local.
