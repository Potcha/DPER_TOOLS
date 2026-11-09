# DPM-3 — diagnostic documentaire de supports linéaires

## Fonction livrée et limites

`inspect_drawing_linear_supports` examine une cote **linéaire** et deux traits droits ouverts explicitement choisis dans une vue PDF courante. Les deux supports doivent être distincts, parallèles à une tolérance angulaire de `1e-6`, séparés de plus de `1e-6` point et avoir un recouvrement projeté de plus de `1e-6` point. Les croisements et contacts dans ce recouvrement sont refusés, même s'ils passent la tolérance angulaire. Les projections utilisent le milieu du recouvrement, sans extrapolation. La séparation est locale à cette projection lorsque les directions diffèrent dans la tolérance numérique.

La réponse conserve la source textuelle, les deux primitives complètes, leurs empreintes et la preuve de contenu de la vue. Une révision périmée, une source PDF modifiée, un support hors vue, un type incompatible ou un contexte documentaire d'unités contradictoire provoque un refus. Une unité absente est explicitement signalée. La valeur nominale ne sert ni au choix des supports, ni au calcul, ni à l'étalonnage.

Ce diagnostic reste `documentary_proposal`, `reviewed=false`, `measured_on_photo=false`. Il ne reconnaît pas les rôles des traits (contour de pièce, ligne de cote, ligne d'attache), ne crée aucune association persistante ni mesure et ne produit aucun verdict physique. Il n'ajoute pas de panneau graphique. DPM-3 reste en cours pour ces associations, les angles et la reconnaissance des vues. Aucun format de projet n'est modifié.

## Recette manuelle fournie avec la livraison

1. Fermer l'ancienne instance de DPER_MEASURE, puis lancer :

   ```powershell
   & 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm3-linear-app\debug\dper_measure.exe'
   ```

2. Importer comme **plan PDF** :
   `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm3-linear-manual\supports-lineaires.pdf`.
   Aucune image ni calibration n'est nécessaire. Le document synthétique contient la cote `18 ±0.1 mm` et quatre traits ; ce n'est pas un plan industriel ni une référence physique. Analyser la page 1 dans « Géométries proposées du PDF » si nécessaire.
3. Demander au chat : **« Exécute la recette DPM-3 des supports linéaires sur le plan ouvert »**. Le chat relève la révision et le plan via `get_inspection_state`, analyse la géométrie de la page si nécessaire puis lit le détail des traits via `get_drawing_geometries`. Il définit explicitement une vue contenant la cote et les quatre traits. Cette préparation ajoute la vue ; les diagnostics suivants restent en lecture seule.
4. Le premier support va de `(40,80)` à `(160,80)` points PDF ; le second de `(60,120)` à `(140,120)`. Leurs identifiants doivent provenir du catalogue courant. Appeler le diagnostic avec la révision renvoyée par `define_drawing_view` :

   ```json
   {
     "expected_revision": 3,
     "feature_id": "IDENTIFIANT_COTE_COURANTE",
     "view_id": "IDENTIFIANT_VUE_COURANTE",
     "support_ids": ["IDENTIFIANT_PREMIER_TRAIT", "IDENTIFIANT_SECOND_TRAIT"],
     "justification": "Paire identifiée explicitement dans le document synthétique ; rôles des traits non reconnus."
   }
   ```

   **Remplacer aussi `expected_revision`** : `3` est un exemple. Les coordonnées servent à identifier les supports de cette fixture ; l'appel reçoit leurs identifiants, pas des coordonnées inventées.
5. Attendre `normal_separation_pt=40`, `overlap_length_pt=80`, projections `(100,80)` et `(100,120)` à `0.01 pt` près. Inverser les deux identifiants : mêmes distances, ordre des projections inversé. Vérifier les sources et empreintes non vides, `documentary_proposal`, les deux booléens à `false` et les raisons de suspension. **40 points PDF ne sont pas 40 mm** ; le nominal reste 18 mm.
6. Vérifier les refus : même identifiant deux fois ; premier support avec le trait oblique `(200,80)–(240,100)` ; premier support avec le trait horizontal disjoint `(200,150)–(240,150)` ; révision précédente ; justification vide ; identifiant inconnu. Les refus doivent expliciter leur raison.
7. Après chaque diagnostic, comparer `get_inspection_state` et `get_measurement_context` au contexte relevé après préparation de la vue : révision, capture, mesures, calibration, historique et données d'inspection doivent rester identiques. Aucun dessin ou écran nouveau n'est attendu. Joindre la réponse MCP et le résultat observé au ticket en cas d'écart.

Le changement externe du fichier est testé automatiquement sur une copie appartenant à la recette ; il n'est pas nécessaire de modifier votre document. Le statut de vue devient alors périmé à la lecture, sans modification des données enregistrées en session.

## Reproduction automatique et résultats exacts

Commande de génération seule, depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE` (choisir un dossier inexistant) :

```powershell
python scripts/verify_linear_drawing_supports_mcp.py output/dpm3-linear-manual-nouveau
```

Le même script avec `--test-port 18771` exécute la recette sur un **hôte de test propriétaire déjà démarré** ; il analyse son PDF synthétique et prépare la vue. Ne pas le diriger vers une session de travail. Le port utilisateur 8765 est volontairement refusé par le script.

Vérifications exécutées sur cette livraison le 9 novembre 2026 :

- `cargo test --locked dper_binding::drawing_linear_supports::` : **9 réussis, 0 échec, 385 filtrés**. Rotations/translations, inversion, sources et nominal, croisements sous tolérance, formes dégénérées, périmètre et source périmés, types et unités, justification.
- `cargo test --locked dper_binding::tests::` : **9 réussis, 0 échec, 385 filtrés**, régressions des associations existantes.
- Transport MCP réel avec extraction du PDF par Poppler : **9 cas réussis** (2 diagnostics et 7 refus, dont PDF modifié). État stocké inchangé ; statut courant de vue recalculé après changement externe. Hôte `inspection_mcp_host_tests::owned_headless_mcp_host` : **1 réussi**, processus arrêté après recette. Port dédié **18771**, serveur utilisateur **8765 non utilisé**.
- `cargo build --locked --target-dir output/dpm3-linear-app` : compilation debug réussie. **40 avertissements de code inutilisé** ; compilation des tests : **30 avertissements**.
- Revue indépendante : cas de croisement presque parallèle identifié, corrigé et couvert par une régression. Deux premières exécutions du script ont corrigé des attentes de recette : pagination résumée sans points, puis statut de vue recalculé après changement externe. La dernière recette passe intégralement.

Preuves locales : `output/dpm3-linear-tests.log`, `output/dpm3-linear-binding-regression.log`, `output/dpm3-linear-build.log`, `output/dpm3-linear-mcp-20261109-235451/recipe/summary.json` et `rpc_evidence_001.json`, avec journaux d'arrêt de l'hôte dans le dossier parent. Les sorties et PDF générés restent locaux ; le générateur reproductible et ce protocole sont livrés. Aucun test physique, qualification optique, conformité ou suite complète n'est revendiqué.
