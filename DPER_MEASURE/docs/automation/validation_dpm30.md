# DPM-30 — Constructions et motifs de mesure

Base : `fc55c9e` (`origin/dev`, PR #38 intégrée). Branche :
`codex/DPM-30-constructions-patterns`. Vérifications du 7 novembre 2026.

## Comportement livré

- Une construction périmée ne sert plus à l'accrochage. Chaque point conserve
  les identifiants et générations des guides réellement utilisés ; une intersection
  conserve ses deux parents. La création refuse un guide modifié entre les clics.
  Annuler un point retire uniquement les dépendances de ce point.
- Les liens sont enregistrés dans `source_measurement_ids` avant l'historisation.
  Modifier un guide invalide donc ses descendants selon le graphe existant.
- Le centre d'une construction permet sa translation sans modifier sa forme.
  Les diagonales visibles des rectangles de construction sont sélectionnables.
- Modifier librement un sommet ou un côté d'un rectangle orienté/arrondi stocké
  en polygone retire explicitement sa variante « Rectangle ». Le message annonce
  la conversion en polygone ; l'annulation restaure la variante d'origine.
  Une translation complète conserve la variante.
- Un pas nul est refusé pour un axe répété. Un axe non répété peut garder un pas
  nul. Le test 4 × 4 vérifie chaque emplacement, y compris les rangées 2 et 4,
  avec deux sources et des pas positifs puis négatifs.

Les copies sont des géométries à ajuster aux pixels. Elles ne deviennent pas des
observations automatiques et ne récupèrent pas l'incertitude de la source.
Les constructions restent exclues des valeurs numériques et de la conformité.
Aucun nouveau format de projet n'est nécessaire : les liens utilisent le schéma
existant et les générations temporaires du tracé ne sont pas sauvegardées.

## Vérifications exécutées

32 tests distincts pertinents réussis, dont 7 nouveaux ; aucune suite complète
relancée conformément à la demande. Les commandes sont exécutées dans le dossier
`O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`.

| Commande après `cargo test --locked` | Résultat |
| --- | --- |
| `recovered_` | 21 réussis, 313 filtrés |
| `recovered_editing_ui_tests::` après correction de la relecture | 10 réussis, 324 filtrés ; déjà compris dans les 21 |
| `patterns::tests::` | 2 réussis, 332 filtrés |
| `measurement_graph::tests::` | 6 réussis, 328 filtrés |
| `construction_guides_remain_visual_and_are_excluded_from_numeric_exports` | 1 réussi, 333 filtrés |
| `vision_and_construction_do_not_inherit_manual_conformity` | 1 réussi, 333 filtrés |
| `csv_and_xlsx_exclude_stale_current_rows_and_keep_manual_root` | 1 réussi, 333 filtrés |

Les tests d'édition exercent les poignées natives, plusieurs mises à jour d'un
glissement, les résultats, l'historique, les liens et les refus applicatifs dans
un contexte egui sans fenêtre. Les tests de compatibilité et de graphe couvrent
la sérialisation des variantes et l'invalidation des dérivés à l'import.
Les exports ciblés vérifient l'exclusion des constructions et valeurs périmées.

Premier lancement : compilation refusée par la visibilité privée de
`guide_center`, corrigée avant exécution des tests. La relecture indépendante a
repéré une alternance de variante entre deux mises à jour du glissement ; la
condition utilise maintenant la géométrie initiale et le test exerce deux mises
à jour avant validation. Les 10 tests d'édition ont ensuite été réexécutés.
28 avertissements de code inutilisé sont signalés par la compilation des tests.
Pas de compilation distincte de l'exécutable, de recette MCP complète, de rendu
PDF, de test caméra/GPU ni de qualification physique revendiqués dans ce lot.

Journaux locaux exclus de la livraison : `output/dpm30-*.log`.

## Validation utilisateur de la PR

1. Ouvrir une image de test. Créer un rectangle en mode Construction, le
   sélectionner, puis déplacer son centre. Vérifier les deux coins et les
   diagonales ; Annuler/Rétablir doit restaurer le déplacement en une action.
2. Créer un cercle dont le centre s'accroche à cette construction. Modifier le
   rectangle : le cercle dérivé doit être signalé à recalculer. Une annulation
   ne rend pas l'ancienne preuve à nouveau courante. Sauvegarder et rouvrir :
   constructions et liens restent présents, les preuves historiques suspendues.
3. Sur une géométrie orientée ou arrondie, déplacer librement un sommet : le
   message doit annoncer un polygone. Annuler restaure le rectangle. Tester aussi
   le déplacement d'une extrémité de segment et du rayon d'un cercle ordinaires.
4. Répéter un tracé sur 4 lignes × 4 colonnes avec des pas non nuls et adaptés à
   l'image : 15 copies plus l'original. Vérifier les rangées 2 et 4. Un pas nul
   sur un axe répété doit être refusé avec un message explicite.

Les valeurs de ces essais sont des données de test. Une construction ou une
copie géométrique ne valide pas la pièce. La qualification physique reste dans
DPM-26 ; le passage complet des tests reste prévu à la fin de la consolidation.
