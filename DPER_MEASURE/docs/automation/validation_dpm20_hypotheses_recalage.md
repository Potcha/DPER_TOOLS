# DPM-20 — Choisir une hypothèse de recalage

Le panneau « Hypothèses des repères circulaires » et
`register_image_circle_landmarks` utilisent la même opération. La recherche
globale conserve les correspondances concurrentes. Chaque candidat possède
une empreinte liée à la capture, à la révision, au PDF, à la vue, au seuil
et aux repères observés. Le choix explicite et sa justification sont conservés
avec les alternatives dans le projet et dans le manifeste du rapport.

## Emplacements Windows

- Dépôt `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`
- Exécutable `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`
- Générateur synthétique `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\scripts\verify_observed_circle_landmarks_mcp.py`
- Client MCP `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\scripts\smoke_inspection_mcp.py`

## Parcours dans l’interface

1. Sur une copie de projet, ouvrir la photo figée, importer le PDF,
   analyser sa géométrie et définir une vue contenant trois à six cercles
   repérables dans la photo.
2. Ouvrir « Hypothèses des repères circulaires » dans l’éditeur de recalage.
   Choisir la vue, cocher les cercles et fixer le seuil en pixels natifs.
   Cliquer « Rechercher les correspondances ».
3. Examiner les correspondances de chaque hypothèse, les résidus et
   « Coordonnées et preuves des contours ». La recherche doit laisser
   le recalage et les mesures dans leur état précédent.
4. Choisir explicitement une hypothèse. Pour un cas ambigu, cliquer
   « Appliquer l’hypothèse choisie » avec une justification vide doit
   demander une explication et conserver la révision.
5. Expliquer le repère qui départage les correspondances, puis appliquer.
   Le recalage doit contenir exactement les correspondances choisies.
   L’historique doit conserver le choix, son empreinte et sa justification.
6. Après application, l’ancienne proposition est périmée. Pour corriger
   le choix, relancer la recherche, examiner les alternatives puis choisir
   explicitement de nouveau. Les anciennes observations dépendantes
   doivent être recalculées.
7. Sauvegarder, rouvrir le projet puis exporter le PDF. L’historique du choix
   et les alternatives doivent rester présents dans le projet et le manifeste.
   Le PDF doit citer le candidat, le nombre d’hypothèses et la justification.

## Rejeu HTTP MCP

Sur un hôte dédié, utiliser le générateur synthétique pour créer le PDF.
La fonction `write_photo` avec `SQUARE` fournit quatre repères symétriques.
La deuxième recherche renvoyée par `prepare_views` décrit leur vue.
Retirer `image_circle_searches` pour appeler la recherche globale.

1. Appeler `propose_image_circle_landmark_matches` avec cette recherche.
   Vérifier quatre hypothèses et quatre entrées `candidate_fingerprints`.
2. Appeler `register_image_circle_landmarks` avec la recherche originale,
   `candidate_id`, son `proposal_fingerprint` et une `justification`.
   Vérifier avant toute application les refus d’un candidat inconnu,
   d’une justification vide, d’une empreinte falsifiée et d’un seuil modifié.
3. Choisir une hypothèse distincte de la première. Vérifier l’identité
   des correspondances avec ce candidat, l’incrément unique de révision,
   la conservation des mesures et de l’étalonnage.
4. Répéter l’ancien appel et vérifier le refus de révision périmée.
5. Relancer la recherche puis choisir une autre hypothèse. Vérifier que
   l’historique comporte les deux choix et leurs alternatives respectives.
6. Sauvegarder et rouvrir le projet. Vérifier l’identité des preuves de choix,
   puis exporter le rapport PDF et vérifier le texte et le manifeste.

Les anciens clients peuvent continuer à appliquer une hypothèse unique
avec `search` et `proposal_fingerprint`. Le choix entre hypothèses concurrentes
exige un identifiant de candidat et une justification.

## Domaine

La recherche porte sur des disques isolés contrastés et une similitude
dans une vue PDF explicite. Le choix documentaire résout une ambiguïté
de correspondances selon l’examen de l’opérateur. La qualification physique
et l’incertitude totale restent nécessaires à la décision de conformité.
DPM-20 reste ouvert pour l’orchestration complète et sa recette finale.

## Résultats exécutés

Sur `codex/DPM-20-registration-alternatives`, base `dev a258c42`,
21 tests d’appariement et 22 tests du panneau du plan ont réussi. Le nouveau
test egui clique une hypothèse parmi quatre, vérifie le refus d’une justification
vide, saisit la justification puis applique le choix. Il vérifie aussi le refus
après changement des pixels, la conservation des alternatives et leur texte
d’export. Le test de recette composée a rencontré l’erreur Windows 650
au premier passage, puis a réussi lors du rejeu hors restriction Windows.

Le rejeu HTTP MCP a été exécuté sur un hôte Windows dédié au port 18785.
Il a vérifié les six refus, le choix d’un candidat différent du premier,
sa correction, les deux historiques, la sauvegarde, la réouverture et
le PDF de trois pages. Les identifiants et empreintes sont comparés exactement.
La comparaison des nombres sauvegardés tolère l’arrondi JSON des flottants
à 1e-10 près, avec une tolérance relative de 1e-12. Un segment synthétique
manuel est ajouté après les choix pour rendre l’image éligible au rapport.
Le choix du recalage conserve les mesures et l’étalonnage existants.

- Journal `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-registration-choices\20261115-212715-312\rpc_evidence_001.json`
- Projet `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-registration-choices\20261115-212715-312\choix.dperproj`
- Rapport `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-registration-choices\20261115-212715-312\choix.pdf`
- Texte extrait `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-registration-choices\20261115-212715-312\choix.txt`

La première page a été rendue et inspectée. Poppler signale la police Symbol,
avec un texte de choix lisible. L’hôte s’est arrêté normalement. La compilation
debug réussit avec 39 avertissements de code inutilisé existants.
La fenêtre Windows interactive reste à vérifier sur cette livraison.
