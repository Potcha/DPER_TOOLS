# DPM-11 — vérifier le contour près des extrémités

Les diagnostics `inspect_straight_edge_endpoints` et `inspect_arc_endpoints` vérifient trois profils à 3, 5 et 7 px vers l’intérieur de chaque extrémité. Chaque profil doit fournir un bord non ambigu à 1,5 px au plus du support ajusté. Le résultat restitue les six points observés dans `interior_observed_points_px`, dans l’ordre début puis fin et du profil le plus proche au plus éloigné. Les profils extérieurs continuent de chercher un prolongement dans les deux polarités.

## Recette Windows

1. Enregistrer le projet courant, fermer l’ancienne instance et lancer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm11-local-support-20261112-084141-764\dper_measure.exe`.
2. Ouvrir les trois images de bord dans `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm11-local-support-20261112-084141-764\straight_edge`, puis les trois images d’arc dans `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm11-local-support-20261112-084141-764\arc`. Pour chaque image, demander « Exécute la recette DPM-11 des preuves intérieures sur l’image ouverte ». Aucun PDF ni étalonnage requis.
3. Le client MCP lit la capture et la révision avec `get_measurement_context` avant chaque appel et utilise les paramètres du script de recette correspondant. Comparer capture, révision, mesures et étalonnage avant et après chaque diagnostic.

| Image | Attendu |
| --- | --- |
| `bord-borne.png` | Segment de 140 px à 0,35 px près et six points intérieurs observés. |
| `arc-borne.png` | Longueur de 94,247780 px à 0,5 px près, ouverture de π/2 à 0,01 rad près et six points intérieurs observés. |
| `bord-prolonge.png`, `arc-prolonge.png` | Refus pour prolongement ou ambiguïté au-delà de l’intersection. |
| `bord-masque.png`, `arc-masque.png` | Refus pour contour intérieur absent, décalé ou ambigu. |

Pour régénérer les entrées dans des dossiers neufs depuis DPER_MEASURE, lancer `python scripts/verify_straight_edge_endpoints_mcp.py output/dpm11-bords` et `python scripts/verify_arc_endpoints_mcp.py output/dpm11-arcs`. L’option `--test-port` exécute le scénario sur un hôte dédié déjà démarré.

## Vérifications exécutées

- `cargo test --locked dper_vision::` a réussi 27 tests, dont deux nouveaux, avec zéro échec et 440 filtrés.
- Les six cas ont été exécutés sur l’hôte MCP dédié 18776. Le segment a donné 140,0000 px. L’arc a donné 94,248831 px et 1,567849 rad. Les quatre refus et l’état inchangé après chaque diagnostic ont été vérifiés. L’hôte a terminé normalement.
- La compilation Windows a réussi dans une sortie distincte après un refus d’accès au binaire de sortie habituel. Elle a émis 39 avertissements. Les scénarios MCP ont utilisé un hôte sans fenêtre.

Les profils sont des preuves locales aux positions sondées. La qualification du contour physique et l’incertitude des extrémités restent à établir.
