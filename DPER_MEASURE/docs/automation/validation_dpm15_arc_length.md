# DPM-15 — Incertitude sur la longueur d’un arc

`calculate_arc_length_uncertainty` propage au premier ordre les incertitudes du rayon en pixels natifs, de l’ouverture en degrés et de l’échelle isotrope en mm/px. Pour `L = r × θrad × s`, les sensibilités sont `θrad × s`, `r × s × π/180` et `L` pour une perturbation relative de l’échelle. L’ordre de la matrice est `radius_px, sweep_deg, relative_scale`. La valeur `0.01` représente 1 % d’incertitude-type relative.

## Recette dans DPER_MEASURE

1. Enregistrer tout projet ouvert, fermer l’ancienne instance et lancer dans PowerShell.

   ```powershell
   & 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm15-arc-app\debug\dper_measure.exe'
   ```

2. Demander dans le chat « Exécute la recette DPM-15 du modèle de longueur d’arc via MCP ». Aucun PDF ni image n’est requis. La fonction `verify(client)` de `scripts/verify_arc_length_uncertainty_mcp.py` exécute les appels en lecture seule. L’option de ligne de commande `--test-port` est prévue pour un hôte de test dédié.
3. Attendre **157,079632679 px** et **1,570796327 mm** pour chacun des trois calculs. À `k = 2`, attendre une incertitude élargie de **0,054413981 mm** pour des entrées indépendantes, **0,062831853 mm** avec une corrélation rayon/ouverture de +0,5 et **0,044428829 mm** avec −0,5.
4. Vérifier les huit refus du script, puis confirmer que le contexte de mesure et l’état d’inspection sont identiques à ceux relevés avant les appels.

### Entrée MCP de référence

```json
{
  "radius_px": 100,
  "standard_uncertainty_radius_px": 1,
  "radius_source": "Rayon synthétique",
  "sweep_deg": 90,
  "standard_uncertainty_sweep_deg": 0.9,
  "sweep_source": "Ouverture synthétique",
  "scale": {
    "millimeters_per_pixel": 0.01,
    "relative_standard_uncertainty": 0.01,
    "source": "Échelle synthétique"
  },
  "coverage_factor": 2,
  "correlation_matrix": [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
  "correlation_justification": "Entrées indépendantes pour cet essai synthétique"
}
```

## Domaine

L’ouverture est une grandeur positive strictement inférieure à 360°. Les arcs majeurs sont admis. Le calcul porte le statut `calculation_only`. Il ne lit pas de contour et ne déduit pas une incertitude à partir des résidus. Le modèle suppose une échelle isotrope et une propagation au premier ordre. Il ne qualifie pas les sources ni l’applicabilité du modèle près des bornes ou à forte incertitude. Le raccordement aux mesures observées et la qualification restent à traiter dans DPM-15.

## Résultats exécutés

- `cargo test --locked --bin dper_measure uncertainty::` a réussi 43 tests, dont trois nouveaux, avec 412 filtrés et zéro échec.
- La recette MCP sur l’hôte de test dédié 18771 a réussi trois calculs et huit refus. Les états de mesure et d’inspection étaient identiques après chaque appel. L’hôte s’est arrêté normalement. Le détail local se trouve dans `output/dpm15-arc-mcp-20261111-094238-521/recipe/summary.json`.
- `cargo build --locked --bin dper_measure` a réussi avec 39 avertissements de code inutilisé. SHA-256 de l’exécutable de recette : `79DE4FC8BDDDA30276737B06A6E946B41F44E1CF86127A50EDBEC8DDED5E9DEC`.
