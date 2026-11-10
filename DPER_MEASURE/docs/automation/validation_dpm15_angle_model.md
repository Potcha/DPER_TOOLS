# DPM-15 — incertitude d’un angle à trois points

`calculate_angle_uncertainty` calcule l’angle mineur entre les deux demi-droites issues du sommet, puis propage au premier ordre les incertitudes des six coordonnées. Il dérive les sensibilités signées en degrés par pixel et utilise le moteur de budget corrélé. La matrice 6 × 6 suit l’ordre `x1, y1, xv, yv, x2, y2`. Chaque point porte deux incertitudes-types en pixels et une source. L’appel exige le facteur d’élargissement et une justification des corrélations.

## Recette manuelle

1. Fermer l’ancienne instance de DPER_MEASURE, puis lancer dans PowerShell :

   ```powershell
   & 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm15-angle-app\debug\dper_measure.exe'
   ```

2. Demander dans le chat **« Exécute la recette DPM-15 du modèle d’angle via MCP »**. Le cas est synthétique et ne requiert aucune image ni aucun plan. Les résultats sont renvoyés par MCP.
3. Vérifier les quatre calculs et les douze refus décrits ci-dessous. Le contexte de mesure et l’état d’inspection sont comparés avant et après chaque appel.

| Cas | Résultat attendu à k = 2 |
| --- | --- |
| Coordonnées indépendantes | Angle 90°, uc = 0,572957795131° et U = 1,145915590262° |
| Corrélation `rho(y1,yv) = 0,5` | Angle 90°, uc = 0,496196005880° et U = 0,992392011759° |
| Permutation des extrémités ou réflexion de la seconde | Même angle et même budget que le cas indépendant |

Pour reproduire le premier cas, appeler `calculate_angle_uncertainty` avec le JSON suivant. Pour le cas corrélé, modifier les cellules `[1][3]` et `[3][1]` à `0.5` (indices à partir de zéro) et mettre à jour la justification. L’exemple applique une tolérance numérique relative de `1e-12` sur le budget.

```json
{
  "first_point": {"position_px":{"x":40,"y":0},"standard_uncertainty_x_px":0.2,"standard_uncertainty_y_px":0.2,"source":"Point synthétique 1"},
  "vertex": {"position_px":{"x":0,"y":0},"standard_uncertainty_x_px":0.2,"standard_uncertainty_y_px":0.2,"source":"Sommet synthétique"},
  "second_point": {"position_px":{"x":0,"y":40},"standard_uncertainty_x_px":0.2,"standard_uncertainty_y_px":0.2,"source":"Point synthétique 2"},
  "coverage_factor":2,
  "correlation_matrix":[[1,0,0,0,0,0],[0,1,0,0,0,0],[0,0,1,0,0,0],[0,0,0,1,0,0],[0,0,0,0,1,0],[0,0,0,0,0,1]],
  "correlation_justification":"Indépendance supposée pour cet essai synthétique"
}
```

Les refus couvrent une demi-droite de longueur nulle, les angles de 0° et de 180°, une incertitude négative, une source vide, un facteur nul, trois matrices invalides (taille, asymétrie, indéfinie) et trois champs inconnus (échelle, nominal, champ ajouté à une position). Le [script de recette](../../scripts/verify_angle_uncertainty_mcp.py) contient les requêtes. Il prépare une petite image de test uniquement sur un hôte dédié. Sur l’instance utilisateur, exécuter les mêmes calculs en lecture seule à partir de l’état déjà ouvert.

La réponse porte le statut `calculation_only` et conserve les entrées, les sensibilités et le budget. Une échelle isotrope commune s’annule dans le calcul d’angle. Le seuil de colinéarité numérique est `|sin(theta)| ≤ 8 × f64::EPSILON` et chaque demi-droite doit dépasser `1e-9 px`. La validité de la linéarisation près d’une configuration dégénérée ou avec de fortes incertitudes reste à évaluer. Le modèle couvre les coordonnées fournies et leurs corrélations. Une application à une mesure réelle exige encore un modèle des sources physiques, notamment de l’anisotropie et de la distorsion si elles sont significatives, ainsi que leur qualification.

## Résultats exécutés

- `cargo test --locked uncertainty::` a réussi les 36 tests ciblés, dont 9 nouveaux, avec 403 tests filtrés. La vérification des dérivées par différences finies couvre les six coordonnées et les deux orientations. Une régression couvre le faux angle de points colinéaires dû à l’arrondi.
- La recette MCP a réussi ses 16 contrôles sur l’hôte dédié 18771. Les quatre calculs et les douze refus ont produit les réponses attendues. Les états comparés après chaque appel étaient identiques. L’hôte a été arrêté normalement.
- La compilation debug a réussi avec 40 avertissements de code inutilisé. Les journaux locaux sont `output/dpm15-angle-tests.log` et `output/dpm15-angle-build.log`. Les requêtes et résultats MCP sont sous `output/dpm15-angle-mcp-20261110-112735/recipe/`.
