# DPM-15 — propagation au premier ordre d'une distance

## Fonction livrée

`calculate_distance_uncertainty` calcule la distance `L = s × hypot(x2-x1,y2-y1)` et ses sensibilités à partir de deux points en pixels et d'une échelle isotrope explicite en mm/pixel. Il réutilise le moteur de budget corrélé existant ; le client ne fournit pas les coefficients de sensibilité.

L'ordre des cinq variables est fixe : **x1, y1, x2, y2, relative_scale**. Les quatre premières incertitudes-types sont en pixels. La cinquième est l'incertitude-type relative sans dimension de l'échelle, sous forme fractionnaire : `0,01 = 1 %`, et non `1`. Le modèle de perturbation de l'échelle est `s × (1 + relative_scale)`, évalué à une perturbation moyenne nulle. Les coefficients signés sont :

`[-s×dx/r, -s×dy/r, s×dx/r, s×dy/r, L]`, avec `r=hypot(dx,dy)`.

La matrice 5×5 complète et la justification des corrélations sont obligatoires, même pour des variables sans contribution dans une orientation donnée. L'identité représente une hypothèse explicite d'indépendance. Les règles numériques du moteur corrélé restent applicables : pas de réparation automatique des matrices, refus des matrices indéfinies/indécidables et des annulations nulles ou trop proches de la résolution numérique.

Le résultat conserve les entrées, les sources, les unités, la matrice, les sensibilités, les contributions, uc et U=k×uc en millimètres. `status=calculation_only` et les quatre indicateurs `applied_to_measurement`, `source_qualification_verified`, `conformity_evaluated`, `input_provenance_verified` restent faux.

## Protocole manuel immédiat

1. Fermer l'ancienne instance et lancer :

   ```powershell
   & 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm15-distance-app\debug\dper_measure.exe'
   ```

   Aucun PDF ni image nécessaire. L'installateur existant ne contient pas ce changement.
2. Demander au chat : **« Exécute la recette DPM-15 du modèle de distance via MCP »**.
3. Le chat relève `get_measurement_context` et `get_inspection_state`, exécute les quatorze cas ci-dessous et compare les deux états après chaque appel. Aucun tracé ni nouvel écran n'est attendu ; les calculs sont dans les réponses MCP.
4. Appeler `calculate_distance_uncertainty` avec :

   ```json
   {
     "first_point": {
       "position_px":{"x":0,"y":0},
       "standard_uncertainty_x_px":0.2,
       "standard_uncertainty_y_px":0.2,
       "source":"Point synthétique 1 ; incertitude de position fournie"
     },
     "second_point": {
       "position_px":{"x":30,"y":40},
       "standard_uncertainty_x_px":0.2,
       "standard_uncertainty_y_px":0.2,
       "source":"Point synthétique 2 ; incertitude de position fournie"
     },
     "scale": {
       "millimeters_per_pixel":0.01,
       "relative_standard_uncertainty":0.01,
       "source":"Échelle isotrope synthétique ; aucune qualification physique"
     },
     "coverage_factor":2,
     "correlation_matrix":[
       [1,0,0,0,0],
       [0,1,0,0,0],
       [0,0,1,0,0],
       [0,0,0,1,0],
       [0,0,0,0,1]
     ],
     "correlation_justification":"Indépendance supposée explicitement pour cet essai synthétique"
   }
   ```

   Attendre **50 px et 0,5 mm**, sensibilités `[-0,006,-0,008,0,006,0,008,0,5]`. Variance `33×10⁻⁶ mm²`, uc=`0,00574456264653803 mm` et U=`0,01148912529307606 mm`. Ce résultat est calculé depuis les coordonnées ; aucune valeur nominale n'est fournie.
5. Refaire avec les variantes suivantes. Tolérance numérique de recette : relative `1e-12`.

| Variante | uc (mm) | U à k=2 (mm) |
| --- | --- | --- |
| Identité, cas précédent | 0,00574456264653803 | 0,01148912529307606 |
| Corrélation x1/x2=0,5, autres hors-diagonales nulles | 0,005617828762075255 | 0,01123565752415051 |
| Corrélations x1/x2=1 et y1/y2=1, échelle indépendante | 0,005 | 0,01 |
| Second point (-40,30), matrice identité | 0,00574456264653803 | 0,01148912529307606 |

   Modifier toujours les deux termes symétriques de la matrice et documenter la nouvelle hypothèse. Dans le troisième cas, le déplacement commun des points s'annule ; la contribution d'échelle reste présente. Dans tous les cas, distance 50 px/0,5 mm et indicateurs d'autorité faux.
6. Vérifier dix refus : points confondus ; échelle nulle ; incertitude de coordonnée négative ; incertitude relative négative ; source de l'échelle vide ; k nul ; matrice de mauvaise taille ; matrice asymétrique ; matrice indéfinie ; champ supplémentaire `nominal`.

   Matrice indéfinie de recette : partir de l'identité, fixer symétriquement rho(x1,y1)=0,9, rho(x1,x2)=0,9 et rho(y1,x2)=-0,9. Le moteur doit refuser ce modèle, sans le réparer. Chaque refus laisse les deux états inchangés.

## Limites et suite du ticket

Cette tranche est un **calcul autonome avec données fournies**, sans lecture des pixels ou de l'étalonnage actif et sans application de U à une mesure native. Les coordonnées, sources et corrélations restent non vérifiées. Il ne s'agit pas d'un budget automatiquement qualifié ou nécessairement complet.

L'approximation est au premier ordre ; sa validité n'est pas évaluée, notamment lorsque les incertitudes de position sont grandes devant la distance. Échelle isotrope uniquement : distorsion, anisotropie, perspective, erreurs non fournies et autres conditions optiques ne sont pas modélisées. Aucune probabilité de couverture n'est déduite de k=2. Une distance inférieure ou égale à 1e-9 px, des coordonnées hors ±1e12 px ou non finies, une échelle non positive et les débordements/sous-flux détectés sont refusés. Les incertitudes-types sont positives ou nulles, mais la combinaison finale doit être exploitable et non nulle.

DPM-15 reste en cours pour le raccordement aux mesures/provenances réelles, les autres mesurandes (diamètres, angles, résultats dérivés) et la qualification. Une valeur calculée ici ne lève aucune suspension de conformité.

## Vérifications réellement exécutées le 10 novembre 2026

- `cargo test --locked distance_uncertainty::` : **9 nouveaux tests réussis**, zéro échec, 421 filtrés.
- `cargo test --locked uncertainty::` : **27 réussis**, zéro échec, 403 filtrés. Ce filtre inclut les neuf nouveaux tests et les dix-huit historiques indépendants/corrélés : **27 tests distincts au total, pas 36**. Pas de suite complète relancée.
- Transport MCP réel sur hôte propriétaire 18771 : **14 cas réussis**, quatre calculs et dix refus. Les états mesure/inspection restent identiques après chaque appel. L'hôte a été exécuté explicitement et arrêté avec succès ; le serveur utilisateur 8765 n'a pas été utilisé.
- Preuves : `output/dpm15-distance-mcp-20261110-110054/recipe/summary.json` et journal RPC dans le même dossier. Le script `scripts/verify_distance_uncertainty_mcp.py` est reproductible sur un hôte de test ≥18765 ; il ouvre une image sentinelle uniquement dans cet hôte avant de relever les états.
- Compilation debug réussie dans `output/dpm15-distance-app`, avec 40 avertissements existants de code inutilisé ; 30 côté tests. Journaux : `output/dpm15-distance-tests.log`, `output/dpm15-distance-related.log`, `output/dpm15-distance-build.log`.
- Relecture indépendante statique sans défaut concret relevé. Aucun essai physique, caméra/GPU ou nouvelle qualification métrologique.
