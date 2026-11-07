# DPM-43 — Dégagement circulaire à centre imposé

Cette fonction mesure le passage limité par des supports droits observés sur l'image. Elle conserve le centre de référence : elle ne déplace pas le cercle pour améliorer le résultat et n'utilise aucun diamètre nominal comme valeur mesurée.

## Méthode livrée

1. Quatre bandes de recherche cycliques permettent d'observer les côtés du rectangle de référence. Les intersections, la convexité, le parallélisme, l'orthogonalité et la cohérence des diagonales sont contrôlés. Un rectangle géométriquement incohérent est refusé.
2. Le centre est construit à l'intersection des diagonales. L'API accepte aussi un centre explicitement fourni, identifié comme tel dans les preuves.
3. Chaque butée déclarée possède une bande de recherche finie et une polarité. Les pixels fournissent son support droit. Le contact le plus proche doit rester dans la portée observée, avec des échantillons acceptés à proximité des deux côtés du contact.
4. Le rayon retenu est la plus petite distance euclidienne entre le centre fixe et ces supports. Les autres contacts conservent leur distance et leur jeu radial. Il n'est pas exigé que quatre butées touchent simultanément le cercle.
5. L'enregistrement exige un aperçu identique, de la même révision, avec les mêmes sources. Il crée un cercle natif éditable, un rectangle de construction, deux diagonales et les supports auxiliaires, avec leurs dépendances. Les preuves conservent séparément les points réellement observés sur chaque droite ; aucun échantillon circulaire n'est inventé.

Les paramètres sont bornés : 2 à 16 butées déclarées, au moins trois directions enveloppantes pour réussir, 256 profils maximum par recherche. Les bandes vides, faibles, ambiguës, de mauvaise polarité ou sans contact local suffisamment observé sont refusées. Les réglages et les limitations restent consultables.

## Interface et MCP

Dans le panneau d'inspection, ouvrir **Dégagement à centre fixe entre butées**. Préparer toutes les constructions de localisation avant de constituer le brouillon : une référence rectangle ou polygone à quatre sommets et une construction Distance par butée. Sélectionner la référence, puis utiliser **Préparer depuis la construction rectangle sélectionnée**. Sélectionner chaque droite et l'ajouter comme butée. Contrôler les coordonnées, l'ordre cyclique et la polarité des recherches ; les segments peuvent être raccourcis pour éviter les coins et les transitions voisines.

Renseigner la caractéristique et un groupe explicite, par exemple `Rangée 1 — passages MEMS`. Cliquer **Prévisualiser le dégagement**, examiner le centre, les points acceptés/rejetés et chaque contact. Enregistrer explicitement uniquement si la prévisualisation convient. Toute modification du brouillon exige un nouvel aperçu ; une modification du document exige une nouvelle préparation.

L'outil MCP `inspect_centered_clearance` exécute la même opération. Il reçoit la capture, la révision, le groupe, l'identifiant logique, les sources natives et une demande en pixels source. Appeler avec `commit: false`, puis utiliser la révision retournée pour la même demande avec `commit: true`. Aucun lien sémantique au PDF n'est implicitement accordé par le nom de caractéristique.

## Groupes, sauvegarde et rapport

Le rapport regroupe les mesures par le groupe réellement conservé dans les enregistrements d'inspection, joint par identifiant natif **et capture**, sans dépendre des libellés. Des appartenances contradictoires ne sont pas résolues arbitrairement. Les constructions parentes accompagnent la vue du groupe, sans devenir des lignes de cote. Les aperçus restent dans la trace d'inspection et sont exclus des mesures numériques.

Le PDF présente d'abord les vues par groupe, puis les preuves des mesures enregistrées. Le manifeste conserve les données complètes, y compris les aperçus, les échantillons et les réglages. Le schéma de projet passe à **18** pour conserver la distinction entre cercle dérivé et contour circulaire. Les anciens projets restent lisibles. Après réouverture, les preuves demeurent historiques et suspendues jusqu'à une nouvelle observation.

## Vérifications logicielles

La recette finale `scripts/test_inspection_workflow.ps1` a exécuté **317 tests Rust sans échec** et compilé l'application en mode debug. L'hôte MCP, ignoré par la suite Rust ordinaire, a été exécuté séparément avec succès sur le port 18766. Le processus de test a été arrêté et le profil utilisateur est resté intact. Le parcours préexistant a passé ses sept cas adverses, ainsi que les recettes PDF de 19 attaches, 11 contextes d'unités et 17 pointes.

`scripts/verify_clearance_mcp.py` a observé huit passages synthétiques dans deux groupes. L'écart maximal du diamètre est de **0,1910734 px** par rapport aux valeurs de la fixture. Les huit enregistrements directs sans aperçu ont été refusés. Les preuves enregistrées reproduisent les aperçus et sont suspendues à la réouverture. L'export refuse 4 001 caractères de notes utilisateur et accepte 4 000 caractères même avec huit observations. Le PDF de 11 pages a été rendu et inspecté. La vue générale et les deux tableaux de quatre mesures précèdent les annexes. Poppler a signalé l'absence d'une police d'affichage Symbol, sans omission visible sur les pages inspectées. Le test physique sur une pièce réelle reste à effectuer.

## Limites et recette utilisateur

- Cette tranche traite des **supports droits locaux**. Les butées courbes, l'étendue réelle des extrémités, les obstacles non déclarés et les faces hors plan ne sont pas mesurés. Une transition isolée ne prouve pas à elle seule qu'il s'agit de matière plutôt que d'un trou ou d'un reflet.
- Les constructions localisent les recherches : elles ne valent jamais mesure validée. La validation logicielle du calcul ne qualifie ni l'optique ni une pièce physique.
- Une calibration isotrope valide convertit le diamètre en mm. Une calibration absente conserve les pixels ; une calibration anisotrope est refusée pour ce diamètre unique. Une échelle obtenue en imposant 4 mm au passage lui-même permet une démonstration relative, sans contrôle indépendant de ce passage.
- La conformité de cette nouvelle méthode demeure **indéterminée** : son association au plan, son domaine de qualification et son incertitude totale restent à établir. Les mécanismes existants d'association manuelle d'une cote à une mesure permettent de présenter le nominal et ses tolérances ; aucune interprétation générale automatique du PDF n'est ajoutée ici.

Pour accepter la livraison logicielle : ouvrir une copie d'image, préparer les constructions, prévisualiser/enregistrer un passage, vérifier les contacts et la butée limitante, créer un second groupe, sauvegarder puis exporter le PDF. Modifier une construction parente et constater la suspension du résultat. Réouvrir la sauvegarde et constater que les preuves restent historiques. Pour accepter la méthode physique : appliquer séparément le protocole DPM-26 avec une référence indépendante et des répétitions sur le montage réel.
