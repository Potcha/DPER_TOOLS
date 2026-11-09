# DPM-11 — vérifier une intersection de deux bords droits

Le diagnostic MCP `inspect_line_intersection` ajuste chaque droite sur ses propres échantillons de pixels natifs. La réponse conserve les preuves par bord, la position de l'intersection, l'angle aigu des supports et la distance entre l'intersection et le point accepté le plus proche de chaque bord. La capture, la révision, l'empreinte des pixels et l'état de qualité accompagnent le résultat.

Le diagnostic refuse un angle inférieur à 15°, un croisement hors image, un contour insuffisant ou un croisement éloigné de plus de 4 px des échantillons acceptés d'un bord. Ces seuils sont des critères logiciels. Le croisement est une construction issue de deux supports ajustés. Les extrémités physiques, les longueurs, l'ouverture des arcs et l'incertitude physique restent à traiter dans DPM-11.

## Recette manuelle sur Windows

1. Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, vérifiez avec `git branch --show-current` et `git rev-parse --short HEAD` la branche `codex/DPM-11-line-intersection` et le commit indiqué dans la PR. Fermez les autres instances de DPER_MEASURE pour utiliser le serveur de cette version. Lancez l'exécutable local indiqué dans la PR, ou reconstruisez cette branche avec `cargo run --locked --target-dir .\output\dpm11-app-build`.
2. Ouvrez le fichier d'essai indiqué dans la PR. Pour le recréer dans un nouveau dossier, lancez `python .\scripts\verify_line_intersection_mcp.py .\output\dpm11-recette-manuelle` depuis le même terminal ou un second. La commande affiche le chemin complet du PNG. Elle utilise Pillow et génère un coin clair sur fond sombre dans une image de 256 × 256 px.
3. Dans le client MCP relié à DPER_MEASURE, appelez `get_measurement_context`. Reprenez les champs `capture_id` et `document_revision` dans cet appel. Les coordonnées sont des pixels natifs de l'image.

```json
{
  "capture_id": "identifiant retourné par get_measurement_context",
  "expected_revision": 0,
  "searches": [
    {"start": {"x": 185, "y": 132}, "end": {"x": 123.5, "y": 132}},
    {"start": {"x": 119, "y": 133.5}, "end": {"x": 119, "y": 195}}
  ]
}
```

4. Envoyez l'appel à `inspect_line_intersection`. Attendez un point à moins de 0,25 px de `(120,5 ; 130,5)`, un angle aigu à moins de 0,25° de 90° et deux ensembles de points acceptés. `endpoint_extent_verified` doit être `false` pour chaque support.
5. Remplacez le premier point de fin par `{"x": 140, "y": 132}` et répétez l'appel. Le bord reste détectable et le croisement est refusé car les observations sont trop éloignées. Vérifiez avec `get_measurement_context` que la liste de mesures, la calibration et la révision sont identiques avant et après. Transmettez le commit et les deux réponses MCP si un résultat diffère.

## Validation automatisée

- `cargo test --locked dper_vision::` a exécuté 19 tests avec succès, dont trois nouveaux tests. Ils couvrent quatre rotations, l'ordre des supports, le bord manquant, le croisement distant, les supports presque parallèles et le hors-image.
- Le script de vérification a aussi exercé le transport MCP sur un hôte de test dédié. Résultat `(120,5000 ; 130,5000) px`, trois refus vérifiés pour un croisement trop éloigné, une capture étrangère et une révision périmée. Les mesures, la calibration et la révision sont restées identiques. L'hôte de test s'est arrêté avec succès.
- Compilation de l'application réussie avec `cargo build --locked --target-dir output/dpm11-app-build` et `CARGO_BUILD_BUILD_DIR` pointant vers le dossier `target`. Le dossier de sortie distinct évite le refus Windows lors du remplacement de l'exécutable dans `target/debug`.
