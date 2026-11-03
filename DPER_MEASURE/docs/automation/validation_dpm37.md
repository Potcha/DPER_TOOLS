# DPM-37 — Hypothèses géométriques de pointes triangulaires

Base : `785584392a0af5c4e3a88457902a040a1d964566` (PR #28 fusionnée). DPM-36 est terminé ; cette livraison correspond à DPM-37, sous-tâche de DPM-3.

## Résultat

Dans une vue PDF explicite, l'application propose une hypothèse de pointe triangulaire lorsque la géométrie vectorielle pleine, ses dimensions et son contact avec un support, un texte et un cercle satisfont les seuils décrits dans [drawing_geometry.md](drawing_geometry.md). La preuve porte le préfixe `geometric_triangular_tip_attachment_hypothesis:` et garde sources, style, empreintes, coordonnées, écarts, seuils et alternatives. Dans l'interface, le plan de contrôle indique « Pointe triangulaire : hypothèse géométrique à vérifier sur le plan. » ; le volet « Sources de la pointe triangulaire » expose le JSON à la demande.

Il s'agit d'une hypothèse géométrique, pas d'une reconnaissance normative de flèche ou de symbole. Les propositions concurrentes restent à vérifier ; aucune association n'est sélectionnée ou revue automatiquement. Ce traitement ne crée ni mesure physique ni calibration.

## Vérifications exécutées

- La recette complète a exécuté **262 tests réussis, zéro échec, un test ignoré**, puis lancé explicitement l'hôte de test : **1 test réussi**. Le build et la préparation de l'exécutable de test ont réussi.
- Quinze tests nouveaux sont couverts : **13 moteur** et **2 interface**. Les filtres ciblés conservés séparément donnent **38 tests `dper_binding` réussis** et **4 tests `pointer_choice_` réussis**.
- **17 PDF synthétiques de pointes** passent par le transport MCP réel. Les **19 cas d'attache** DPM-34/35 et les **11 cas d'unités** DPM-36 restent réussis.
- Le smoke MCP réussit avec **18 outils découverts** et **7 cas adversariaux**. La recette native et la parité géométrie/preuve/valeur/résultat avec MCP sont confirmées.
- `isolation_check.json` confirme l'arrêt du processus de test sur le port **18766** et un profil utilisateur inchangé. Aucun service utilisateur sur 8765 n'est concerné.
- Avertissements consignés : `tests.log` rapporte **13 warnings** pour le binaire de test ; `build.log`, **18 warnings** pour le binaire d'application ; `test-build.jsonl` contient **13 diagnostics de niveau warning**. Les comptes tests/build comprennent respectivement 12/17 avertissements de code existants et le repli du cache incrémental vers copie. Les filtres ciblés rapportent chacun **13 warnings**, avec le même diagnostic de cache.

## Corrections rencontrées pendant la recette

La première compilation ciblée a révélé une annotation de type `f64` manquante dans le parseur de coordonnées ; elle a été ajoutée. Huit tests ont ensuite échoué parce que le SVG auxiliaire de contrôle de provenance utilisait une dimension de `1e9` points, refusée par l'extracteur ; le repère auxiliaire a été rendu valide et identitaire, avec une régression utilisant le chemin Poppler réel.

Un test de cercle comparait un résidu flottant à zéro exact : le fit transformé produisait `1.7763568394002505e-14` point PDF. L'assertion du test seulement accepte désormais une marge d'arrondi de `1e-10` point PDF ; aucun seuil de production n'a changé. Le premier fixture `base_gap` plaçait son extrémité sur le segment de base ; il a été déplacé hors du segment, et les **17 cas** passent ensuite.

La première tentative de recette complète, dans l'environnement restreint, a été bloquée par l'accès loopback à `127.0.0.1:18766`. La tentative autorisée suivante a exposé le fixture `base_gap` incorrect ; la recette finale `dpm37-run-03`, avec l'accès loopback au serveur de test dédié, a réussi.

Après le contrôle visuel initial des **17 pages rendues sur trois planches**, le coordinateur a relevé un seul défaut : le libellé du fixture `reflected` chevauchait son trait et son cercle. Son chemin de renvoi a été corrigé pour garder le texte horizontal à part ; triangle, cercle et reflet restent identiques. La recette complète a alors été relancée dans le dossier neuf `dpm37-run-04` et réussit à nouveau. Cette correction concerne uniquement la lisibilité de la fixture, sans changement de production.

## Preuves et limites

Résultats de la recette finale : `output/delivery/DPM-3-triangular-tips/snapshot/DPER_MEASURE/output/automation-validation/dpm37-run-04/` (`tests.log`, `build.log`, `test-build.jsonl`, journaux MCP/PDF, `tip-checks/tips_summary.json`, preuves natives et `isolation_check.json`). Filtres ciblés : `output/delivery/DPM-3-triangular-tips/checks/binding-04.log` et `native-tips-01.log`.

Contrôle visuel terminé : 17 pages d'entrée rendues sur trois planches et inspectées, sans coupure visible. Après la correction du cas `reflected`, les 16 autres sources PDF ont été vérifiées identiques octet pour octet aux pages déjà inspectées ; la page modifiée a été rendue à nouveau et inspectée. Le texte corrigé est séparé du trait et du cercle. Les petits écarts géométriques sont vérifiés numériquement, sans qualification visuelle de précision. Planches locales : `output/delivery/DPM-3-triangular-tips/pdf-qa/`. Aucun nouveau contrôle visuel de rapport ou de fenêtre de bureau/GPU n'est revendiqué.

Répartition appliquée à la finalisation après la demande de routage : GPT-6.1 Sol, effort Medium, pour le moteur et sa relecture ; GPT-6 Luna, effort Medium, pour la documentation et l'exécution des contrôles ; coordination et revue d'intégration par le coordinateur.

Les validations reposent sur des fixtures PDF, des tests d'interface et un transport MCP synthétique. Elles ne qualifient aucune pièce, caméra, chaîne optique, calibration physique ou conformité. DPM-3 reste ouvert.
