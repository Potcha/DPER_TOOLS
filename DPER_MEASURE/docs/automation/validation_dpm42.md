# DPM-42 — récupération du code de l’interface d’origine

## Provenance et périmètre

Le code provient de `DPER_MEASURE_avant_sync_20261105_194955/fichiers_locaux.zip`,
sauvegarde locale du 5 novembre 2026 conservée intacte. L’empreinte SHA-256 du
`main.rs` brut est `d6d28e18b1b88e5cec1545e99b42e23ac7dd9bce7032e428d61eb6e816d23de0`.
Les captures utilisateur servent à reconnaître les éléments attendus ; elles
n’ont pas servi à redessiner l’interface.

Le présent lot remplace le cadre adapté de la PR #33 par les implémentations
d’origine : ruban, sous-menus, messages, timeline fonctionnelle, panneau
Mesures/Annotations, cartouches, flèches, grille, axes, constructions, sélection,
poignées, déplacement groupé, annulation et rétablissement. Les PNG des onglets
restent les images retrouvées dans cette sauvegarde.

Les blocs de l’ancien `main.rs` ont été répartis dans `recovered_chrome_ui`,
`recovered_results_ui`, `recovered_measurement_ui`, `recovered_editing_ui` et
`recovered_workspace_actions`. Les variantes, motifs et primitives graphiques
proviennent de `drawing_variants`, `drawing_ui`, `patterns`, `selection_geometry`
et `measurement_graphics` de la même sauvegarde. Le manifeste
`evidence/dpm42.json` indique empreintes, correspondances et lignes d’origine.
Les modules complets `ribbon`, `ui_widgets`, `drawing_variants`,
`selection_geometry` et `measurement_graphics` correspondent à leur source
après normalisation des fins de ligne.

## Raccordements nécessaires au code actuel

Les services récents d’inspection, de PDF, de recalage et de rapport sont
conservés. Les adaptations concernent la visibilité des méthodes déplacées,
les protections de sélection photo, l’invalidation des dépendances et les
valeurs suspendues. Les mesures natives conservent l’incertitude de leur profil.
Les copies de motifs sont liées à leurs sources ; elles ne transfèrent aucune
preuve vision ni incertitude physique par simple duplication.

Le projet écrit le schéma **17**, pour conserver `drawing_variant` et empêcher
un ancien exécutable de supprimer silencieusement cette information. Les
anciens projets sans ce champ restent lisibles. Les hôtes de test sans services
ne chargent plus le profil utilisateur, dont la migration pouvait écrire un
fichier. L’application normale charge toujours ses réglages.

## Vérifications exécutées

- **297 tests Rust réussis**, zéro échec ; un hôte ignoré par défaut, puis
  exécuté explicitement dans la recette MCP.
- **15 nouveaux tests de récupération** : rayon de cercle, extrémité et direction
  du segment, cartouche indépendant, multisélection et historique, constructions,
  incertitude et dépendances, anciens champs et rectangle orienté ; clics réels
  egui sur la timeline et refus pendant un geste en cours.
- Compilation debug réussie : **37 avertissements** ; tests : **28**. Il s’agit
  principalement de code mort, incluant des composants historiques récupérés ;
  ils ne sont pas présentés comme tous préexistants.
- Parcours MCP réel : **18 outils**, **7 cas adverses**, projets,
  sauvegarde/rechargement, historique, invalidation et rapports réussis.
- Extraction PDF via MCP : **19** cas d’attache, **11** cas d’unités,
  **17** cas de pointes réussis.
- Profil utilisateur inchangé ; processus de test propriétaire du port **18766**
  arrêté ; serveur utilisateur **8765** non utilisé.
- Cinq captures eframe natives inspectées : Caméra, Mesures, Calibration,
  Inspection et Image. Contrôle des cartouches cyan/orange, poignées, flèches,
  diagonales violettes, panneau droit et ordre Longueur connue / Valider / Annuler.
  Données synthétiques, sans caméra ni qualification physique.

Journaux : `output/dpm42-original-validation-04/` ; captures :
`output/design-recovery/original-ui-review-02/`. Ces sorties restent locales.
Deux sous-agents ont préparé des tests indépendants et relu les raccordements.
Les protections de clic photo, d’acquisition et de dépendances relevées pendant
la revue ont été corrigées avant la recette réussie.

Les premiers essais ont détecté une attente de schéma restée à 16 et un fixture
qui changeait directement l’outil sans sa variante. Ils ont été corrigés, puis
la recette complète a réussi. Une capture intermédiaire a révélé qu’un wrapper
de désactivation déplaçait la liste de résultats sous le panneau : corrigé,
puis capture native et recette répétées avec succès.

## Réception utilisateur

Dans `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, lancer `cargo run --locked`.
Utiliser une copie d’un projet ou une image d’essai de qualité suffisante.

1. Vérifier les neuf onglets et leurs images, les menus de groupes et les
   sous-menus Cercles/Rectangles. Réduire la fenêtre et vérifier le menu Plus.
2. Cliquer les étapes de la timeline : Capture ouvre son menu ; Plan ouvre
   l’import ; Étalonnage démarre sa méthode ; Mesures active Distance ; Contrôle
   ouvre le plan existant ; Export prépare les options du rapport. Terminer ou
   annuler les gestes en cours avant de changer d’étape.
3. Créer un rectangle en mode Construction : diagonales violettes et centre
   doivent apparaître. Utiliser l’accrochage Constructions pour centrer un cercle.
4. Passer en Sélection : tirer une poignée du cercle pour changer son rayon,
   son centre pour le déplacer ; modifier une extrémité du segment pour changer
   sa direction. Tirer uniquement le cartouche : les points doivent rester fixes.
5. Sélectionner plusieurs objets avec Maj/Ctrl ou un cadre, déplacer l’ensemble,
   puis Annuler/Rétablir. Retrouver les messages, couleurs et annotations.
6. En étalonnage, ajuster les points avant validation ; vérifier l’ordre de la
   longueur et des boutons. Enregistrer sous un nouveau nom puis rouvrir le projet.
7. Dans Inspection, retrouver les éditeurs PDF/recalage et les preuves récentes.
   Modifier une source liée : les descendants périmés doivent perdre leurs
   valeurs courantes, y compris dans les rapports.

Cette réception porte sur l’interface récupérée. Le ticket DPM-30 reste à
examiner pour sa couverture complète des motifs et exports ; la qualification
physique DPM-26 est distincte.

Pour reproduire la recette logicielle :

```powershell
cargo test --locked recovered_
./scripts/test_inspection_workflow.ps1 -OutputDirectory output/recette-dpm42-nouvelle
```

Choisir un nouveau dossier de sortie. Aucun worktree n’a été créé.
