# Ruban DPER_MEASURE — restauration DPM-29

Le ruban reprend le travail local antérieur inspiré de l'organisation SolidCraft :
neuf onglets, commandes en tuiles, menus sous les groupes, sous-menu Cercles et
débordement Plus lorsque la fenêtre est étroite. Il est adapté au code actuel de
`dev`, base `74bb9d6b9845ed4f272fac342528cc87da6ed140`, sans remplacer les modules
récents d'inspection, de validation ou de rapports par leurs anciennes versions.

## Ressources retrouvées

Les PNG proviennent de la sauvegarde locale réalisée avant synchronisation du
5 novembre 2026 (`DPER_MEASURE_avant_sync_20261105_194955/fichiers_locaux.zip`).
Les treize PNG retenus sont identiques, octet par octet, à cette sauvegarde.
Les fichiers originaux et leurs variantes transparentes sont conservés.
`assets/ui/ribbon_tabs/manifest.json` décrit les cellules et leurs empreintes SHA-256.
Aucun nouveau dessin n'a remplacé les icônes PNG des onglets.

`src/ribbon_ui.rs` embarque une seule texture `atlas_onglets.png` avec
`include_bytes!`, puis sélectionne une cellule par onglet. `ui_widgets.rs` peint
cette icône dans un vrai bouton egui avec texte, survol, focus et sélection.
Les pixels sont des références et des ressources graphiques : les menus,
onglets et champs restent des contrôles natifs et interactifs. Les petits
pictogrammes des commandes réemploient le composant historique `ui_widgets`.
Aucun code ou visuel de SolidCraft n'est inclus.

## Organisation restaurée

| Onglet | Accès rétablis |
| --- | --- |
| Caméra | Actualiser, démarrer/figer/reprendre, réglages et formats existants |
| Capture | Photo, vidéo, série, menu avec retardateur et paramètres time-lapse |
| Fichiers | Image, projet, recherche et enregistrement |
| Calibration | Méthodes, longueur connue, validation et annulation |
| Mesures | Sélection, distances/angles, formes, sous-menu Cercles, coordonnées, historique |
| Inspection | Import du plan, parcours guidé, comparaison et panneau de référence/recalage |
| Image | Corrections optiques, distorsion, empilement et panorama |
| Exports | Rapports PDF/PPTX, données, images et CAO existants |
| Automatisation | Serveur MCP et réglages existants |

La carte Longueur connue précède Valider puis Annuler. La visionneuse occupe
l'espace principal, avec zoom, grille, axes et historique dans sa barre sombre.
Le bouton Configuration ouvre les réglages détaillés conservés à gauche.
Dans Inspection, le panneau de référence et les éditeurs actuels apparaissent à
droite si Configuration est fermé. La liste de résultats est défilante.
Le parcours en six étapes permet d'accéder aux onglets ; ses marques vertes
indiquent la présence d'une image, d'un plan ou d'un étalonnage, jamais une
conformité physique.

## Compatibilité et périmètre

Les commandes appellent les méthodes existantes : aucune géométrie dessinée
ne devient une mesure validée. L'invalidation des dépendances reste exécutée
même lorsque le panneau Configuration est fermé. Le choix des repères photo
conserve ses protections ; son bouton d'annulation reste accessible.
La commande Sélection fige le flux avant l'édition, comme le contrôle existant.

Le travail DPM-4 a été sauvegardé séparément sur sa branche avant cette reprise.
Les compléments de constructions, variantes et motifs sont suivis dans DPM-30 ;
ils ne sont pas remplacés par des commandes fictives dans cette livraison.
La sauvegarde complète du travail de conception reste intacte et locale.

## Vérification locale de l'interface

Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, démarrer `cargo run --locked`.

1. Parcourir les neuf onglets et vérifier leurs PNG, le survol et l'onglet actif.
2. Dans Mesures, ouvrir Formes et contours puis Cercles et sélectionner Cercle ajusté.
3. Réduire la fenêtre : Plus donne accès aux groupes masqués ; les onglets défilent.
4. Ouvrir une image de test, démarrer Calibration : Longueur connue, Valider,
   Annuler doivent apparaître dans cet ordre, avec validation désactivée sans points.
5. Dans Inspection, retrouver les éditeurs PDF/recalage actuels à droite.
6. Ouvrir Configuration puis le refermer ; vérifier l'espace rendu à la visionneuse.

`--ui-review NOUVEAU_DOSSIER` est un mode explicite de capture native sur une
image synthétique, sans démarrage caméra ni serveur MCP. Pour isoler aussi la
lecture du profil, définir APPDATA vers un profil de test avant son lancement.
Les captures et journaux de recette restent dans `output/`, exclus de Git.
Les résultats effectivement exécutés figurent dans
`docs/automation/validation_dpm29.md` et sa preuve machine.
