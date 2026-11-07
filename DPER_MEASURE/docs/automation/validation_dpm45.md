# DPM-45 — Priorité des poignées de redimensionnement

Base : `ddeb4a7` (dev après PR #39). Retour utilisateur sur l'installation
DPM-31 : extrémités de segments et coins de rectangles parfois difficiles à
redimensionner ; cercle, centre du rectangle, historique et réouverture OK.

## Correction

La recherche choisissait d'abord une poignée, puis pouvait la remplacer par
une projection plus proche sur le trait voisin. Près d'une extrémité, cela
déclenchait une translation ; près d'un coin, un déplacement sur un seul axe.

Pour les segments et rectangles, une poignée explicite possède maintenant sa
zone de clic de 12 points écran egui. Les projections sur les traits restent
disponibles hors de ces zones. Cette priorité s'applique aussi lorsqu'un
cartouche chevauche une poignée. Hors chevauchement, le cartouche reste déplaçable.
Le traitement des cercles est inchangé. Aucun changement de format de projet,
de calibration, de méthode de mesure ou d'invalidation des dépendances.

## Vérifications ciblées

Commande : `cargo test --locked recovered_editing_ui_tests::`.

Résultat final : **15 réussis, 0 échec, 324 filtrés**, dont 5 nouveaux tests.
Compilation des tests réussie, avec 28 avertissements de code inutilisé.
Journal local : `output/dpm45-handles.log`.

Les cinq tests nouveaux couvrent les deux extrémités et les quatre coins,
quatre ordres de stockage des coins, des décalages de clic de 2/5/8 points dans
huit directions, deux facteurs d'affichage (2 et 8) et un déplacement d'image.
Ils exercent le circuit natif d'édition dans un contexte egui sans fenêtre :
déplacement sur les deux axes, ancrage opposé, translation centrale, projections
hors poignées et rayon du cercle. Les dix tests existants liés à cette édition
vérifient notamment cartouches, historique, sélection et constructions.

Premier lancement : compilation refusée par une mise en forme Debug absente
dans les messages des nouveaux tests, corrigée sans modifier le type applicatif.
Lancement suivant : 14 réussis et un échec révélant le chevauchement d'un coin
par un cartouche. Après correction de cette priorité : les 15 tests réussissent.
Relecture indépendante statique : pas de défaut supplémentaire identifié.
Pas de suite complète, de test du pilote graphique ni de nouveau build
d'installateur exécutés pour ce correctif.

## Rapport utilisateur examiné séparément

Le PDF utilisateur a été lu et ses 13 pages rendues ; planche-contact examinée,
puis pages 11/12 inspectées en détail. Deux groupes présentent respectivement
M7 = 2,058 mm et M11 = diamètre 1,670 mm. La construction auxiliaire n'est pas
une ligne numérique. L'absence de cote de référence associée est explicite.
Les premières pages sont majoritairement consacrées à l'audit et aux limites ;
cet examen ne valide pas la précision physique. Avertissement Poppler sur la
police Symbol lors du rendu. PDF et images utilisateur restent locaux, hors Git.

## Recette du correctif livré

Sur une version contenant cette PR, sélectionner le segment puis tirer chaque
pastille d'extrémité avec un clic légèrement décentré : seule l'extrémité visée
doit changer. Tirer ensuite chaque coin d'un rectangle en diagonale : les deux
dimensions doivent changer avec le coin opposé fixe. Vérifier les pastilles
centrales, les cartouches et le cercle, puis Annuler/Rétablir. La saisie numérique
par double-clic reste une évolution distincte, DPM-44.
