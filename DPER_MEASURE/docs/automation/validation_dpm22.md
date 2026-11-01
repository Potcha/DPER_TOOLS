# DPM-22 — Consultation native des géométries PDF (incrément partiel)

Livraison du 1er novembre 2026, basée sur la PR #18 fusionnée. Le panneau « Géométries proposées du PDF » permet de choisir une page reconnue, lancer l'extraction en arrière-plan, filtrer les familles et consulter vingt propositions par liste avec leur provenance. Une seule analyse native est active ; le résultat passe par les mêmes contrôles de révision, source et installation que le MCP.

Le cache de présentation suit la révision, les métadonnées source, les installations de catalogues, la page, le filtre et la pagination. Trois pages sont conservées par session. Après rechargement ou éviction, une nouvelle analyse est requise. Les primitives restent des propositions en points PDF : aucune mesure, calibration ou confirmation d'association n'est créée.

## Validation exécutée

- 142 tests Rust réussis, zéro échec. Quatre nouveaux tests couvrent empreinte/page, rendu egui sans services caméra/MCP, installation sans mutation métrologique, résultat périmé et invalidation de cache lors d'une installation partagée sans changement de révision.
- Compilation debug réussie, 18 avertissements existants.
- La première exécution comptait deux échecs de fixture : des dimensions SVG sans unité donnaient 75 points au lieu de 100. La fixture utilise désormais des points explicites ; les gardes de production restent inchangées.
- Revue indépendante du code effectuée. La recette graphique interactive reste à effectuer.

Le parcours MCP n'a pas pu démarrer : le processus graphique n'a pas exposé son port dans le délai de lancement, y compris après une attente locale portée à 200 tentatives de disponibilité. Aucune réussite MCP de cette livraison n'est revendiquée. Les contrôles d'isolation sont conservés.

## Recette utilisateur et périmètre restant

Importer un PDF vectoriel dans Inspection, ouvrir le panneau du plan de contrôle, puis « Géométries proposées du PDF ». Choisir une page, lancer l'analyse, vérifier le compteur et développer une primitive pour consulter sa boîte, son éventuel cercle et sa source. Tester filtre et pagination, puis réimporter/recharger : le panneau doit demander une réanalyse. Une source image ou une page sans vecteurs doit expliquer cette limite sans créer de mesure.

Le recalage, la définition des vues, la sélection des associations et l'observation guidée restent pilotés par MCP : DPM-22 reste ouvert. Une modification externe qui conserve taille et date peut laisser un affichage en cache ancien ; les opérations gardées vérifient toujours le contenu réel. Le rendu sans fenêtre ne remplace pas une recette interactive, et aucun essai ne qualifie une pièce réelle.

La [preuve machine](evidence/dpm22.json) conserve les empreintes et les résultats. Journaux locaux : `output/delivery/DPM-22-vector-ui/`. Les données générées sont synthétiques et restent locales.
