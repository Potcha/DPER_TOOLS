# DPM-7 — hypothèses de correspondance entre repères explicites

## Portée

`propose_landmark_matches` compare deux ensembles complets de même taille, de 3 à 6 points nommés. Toutes les permutations sont évaluées avec le moteur existant de similitude directe (translation, rotation, échelle uniforme), soit au maximum 720 possibilités. Chaque point doit respecter le seuil en pixels image ; aucun point aberrant ne peut être écarté pour accepter une hypothèse. Le contrôle RMS existant, limité à 0,8 fois le seuil, reste applicable.

Le résultat conserve les points fournis, les libellés de source, les correspondances, les résidus des ajustements disponibles et les motifs des permutations refusées. Une hypothèse unique reste une proposition à examiner. Plusieurs hypothèses admissibles donnent `ambiguous`, sans sélection par le meilleur résidu. Les identifiants `candidate-NNN` sont locaux à la réponse et dépendent de l'ordre des entrées.

Cette tranche ne détecte pas les repères dans un PDF ou une photographie. Les coordonnées et libellés de source fournis ne sont pas vérifiés : `registered`, `reviewed` et `input_provenance_verified` restent faux. Aucun recalage n'est appliqué, aucune calibration ou mesure n'est créée. Pas de réflexion, perspective, échelle anisotrope, point manquant ou supplémentaire. DPM-7 reste ouvert pour la détection et le raccordement aux sources natives vérifiées.

## Validation manuelle immédiate

1. Fermer l'ancienne instance et lancer :

   ```powershell
   & 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm7-landmark-app\debug\dper_measure.exe'
   ```

   Aucun PDF ni image n'est nécessaire. L'installateur existant ne contient pas cette évolution.
2. Demander au chat : **« Exécute la recette DPM-7 des correspondances de repères via MCP »**. Le chat relève `get_measurement_context` et `get_inspection_state`, exécute les cas ci-dessous, puis vérifie les deux états après chaque appel. Aucun nouveau tracé ni écran n'est attendu.
3. Appeler `propose_landmark_matches` avec :

   ```json
   {
     "plan_landmarks": [
       {"id":"p0","position":{"x":0,"y":0}},
       {"id":"p1","position":{"x":40,"y":0}},
       {"id":"p2","position":{"x":10,"y":30}}
     ],
     "image_landmarks": [
       {"id":"i2","position":{"x":40,"y":70}},
       {"id":"i0","position":{"x":100,"y":50}},
       {"id":"i1","position":{"x":100,"y":130}}
     ],
     "plan_source":"Repères synthétiques explicites du plan, sans échelle physique",
     "image_source":"Repères synthétiques explicites de l'image, sans détection des pixels",
     "threshold_px":0.01
   }
   ```

   Attendre `unique_hypothesis`, 6 permutations, 1 acceptée et 5 refusées. Les correspondances attendues sont p0/i0, p1/i1, p2/i2 malgré l'ordre différent des listes. La similitude analytique a une échelle 2, une rotation +π/2 et une translation (100,50), à 1e-9 près. Les résidus sont proches de zéro. Les trois indicateurs d'autorité restent faux.
4. Refaire avec un carré plan (0,0), (20,0), (20,20), (0,20), et ses points image obtenus par `x'=100-2y`, `y'=50+2x`. Attendre 24 permutations, 4 acceptées, 20 refusées, `ambiguous` et absence de `unique_candidate_id`. Aucun choix automatique entre les quatre rotations.
5. Refaire le triangle en changeant le signe des x image : attendre `no_match`, les 6 permutations refusées. Puis utiliser quatre points plan (0,0), (40,0), (10,30), (70,25), transformés par la même formule, en ajoutant 200 au x du dernier point image : attendre `no_match`, 24 permutations refusées.
6. Refaire le triangle initial avec seuil 0,5 et +0,05 au x de i2 : attendre une seule hypothèse, avec tous les résidus dans le seuil. Consulter les preuves ; ne pas transformer ce résultat en preuve de correspondance physique.
7. Vérifier les sept refus d'entrée : effectifs différents ; seulement deux points ; identifiant plan dupliqué même avec espaces ; positions image identiques ; source vide ; seuil nul ; champ supplémentaire `reviewed:true`. Chaque appel doit échouer sans modifier la session.

## Recette reproductible et résultats du 10 novembre 2026

Le script `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\scripts\verify_landmark_matches_mcp.py` utilise exclusivement un hôte de test propriétaire sur un port ≥18765. Il crée une image sentinelle dans cet hôte pour exercer sa boucle applicative ; cette préparation précède les comparaisons d'état. Ne pas le lancer contre le serveur utilisateur 8765.

Exécution réelle : `output/dpm7-mcp-20261110-102800/recipe/summary.json` conserve les 12 cas, entrées et réponses ; les échanges RPC sont dans le même dossier. **12 réussis, zéro échec, contexte mesure et inspection inchangés après chaque appel.** L'hôte 18771 a été exécuté explicitement puis arrêté avec succès ; le serveur utilisateur 8765 n'a pas été utilisé.

- `cargo test --locked landmark_matching::` : 9 réussis, zéro échec, 403 filtrés. Cas asymétriques, symétries, réflexion, aberrant, bruit, exhaustivité jusqu'à 6!, entrées invalides et indicateurs d'autorité couverts.
- `cargo test --locked dper_registration::` : 7 réussis, zéro échec, 405 filtrés. Soit **16 tests Rust distincts**, plus le test de l'hôte MCP.
- Compilation debug réussie dans `output/dpm7-landmark-app`, 40 avertissements de code inutilisé existants ; 30 lors de la compilation des tests.
- Journaux : `output/dpm7-landmark-tests.log`, `output/dpm7-registration-related.log`, `output/dpm7-landmark-build.log`.
- Relecture indépendante statique sans défaut concret relevé. Aucune suite complète relancée, aucun essai physique ni contrôle caméra/GPU. Les exemples sont synthétiques.

La recette établit le fonctionnement logiciel du comparateur, pas une reconnaissance automatique ou une qualification métrologique de pièce.
