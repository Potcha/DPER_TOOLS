# DPM-11 — extrémités visibles des arcs

## Fonctionnement et limites

`inspect_arc_endpoints` ajuste l'arc et deux limites droites indépendamment sur les pixels. Les intersections doivent être uniques, proches des observations des deux supports et des extrémités recherchées (4 px maximum), dans l'image et avec un angle d'au moins 15°. L'ouverture doit rester cohérente avec l'arc observé, atteindre le seuil configuré (60° par défaut) et représenter au moins 8 px de longueur. Le sens signé et le passage à travers zéro sont conservés ; le complément d'un arc ne peut pas remplacer l'ouverture observée. Le complément doit laisser plus de 14 px pour les contrôles extérieurs.

Trois profils radiaux de rayon de recherche 2 px, à 3, 5 et 7 px au-delà de chaque extrémité, recherchent une continuation dans les deux polarités. Un profil ambigu, hors image ou contenant un bord provoque un refus. La réponse contient les deux points, l'ouverture signée en radians, la longueur d'arc en pixels, les trois ajustements et les positions des profils extérieurs.

Ce diagnostic reste en lecture seule : aucune mesure, calibration ou conformité n'est créée. Il ne prouve pas une étendue physique cachée et ne calcule pas d'incertitude métrologique. Le champ `endpoint_extent_verified` des preuves de support reste faux. Les coordonnées sont des pixels natifs, jamais des coordonnées écran. Une limite courbe, masquée ou non observable est hors du domaine de cette tranche.

## Recette dans l'application

1. Fermer les autres instances pour libérer le port MCP 8765. Lancer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm11-arcs-app\debug\dper_measure.exe` (branche `codex/DPM-11-arc-endpoints`, commit indiqué dans la PR).
2. Ouvrir **Fichiers → Image…** puis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm11-arcs-mcp\fixtures\arc-borne.png`.
3. Demander dans ce chat : « Exécute la recette DPM-11 des arcs sur l'image ouverte ». Le chat relève `get_measurement_context`, puis appelle `inspect_arc_endpoints` avec les paramètres ci-dessous, la capture et la révision courantes.
4. Attendre des points proches de (188,5 ; 128,5) et (128,5 ; 188,5) px, une ouverture proche de +π/2 et une longueur proche de 94,247780 px. Marges de cette recette synthétique : 0,35 px par point, 0,01 rad sur l'ouverture, 0,5 px sur la longueur. Consulter les trois preuves de contour et les six profils extérieurs. Aucun tracé n'est ajouté automatiquement à l'écran.
5. Ouvrir `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm11-arcs-mcp\fixtures\arc-prolonge.png` et refaire la demande. La continuation, même de polarité inversée, doit être refusée avec « L'arc se prolonge ou reste ambigu au-delà de sa limite ».
6. Pour chaque appel, comparer les contextes avant/après : capture, révision, calibration et liste des mesures doivent rester identiques. En cas d'écart, transmettre le nom du PNG, le commit et la réponse MCP. Ces essais ne valident pas une pièce réelle.

Les fichiers générés restent locaux. Pour les recréer dans un nouveau dossier : `python .\scripts\verify_arc_endpoints_mcp.py .\output\dpm11-arcs-manuel` depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`. Pillow est nécessaire. Le générateur utilise un suréchantillonnage déterministe 4 × 4.

## Paramètres MCP

Après chaque ouverture, reprendre `capture_id` et `document_revision` dans `capture_id` et `expected_revision`. Le champ `search` est :

```json
{
  "center": {"x": 129.1, "y": 127.9},
  "radius": 61,
  "start_rad": 0.05,
  "sweep_rad": 1.4707963267948965,
  "start_boundary": {
    "start": {"x": 185.5, "y": 127.2},
    "end": {"x": 158.5, "y": 127.2}
  },
  "end_boundary": {
    "start": {"x": 127.2, "y": 158.5},
    "end": {"x": 127.2, "y": 185.5}
  }
}
```

Le rayon et les angles fournis localisent la recherche ; ils ne sont pas utilisés comme valeurs mesurées.

## Vérifications exécutées

- `cargo test --locked dper_vision::` : **25 réussis**, zéro échec, 353 filtrés. Trois nouveaux tests couvrent quatre rotations, les deux sens de parcours, le passage angulaire à zéro, les limites absentes, la continuation inversée, l'arc trop court, les intersections concurrentes et les tangentes.
- Transport MCP réel sur hôte dédié 18771 : **94,248831 px**, ouverture **1,567849 rad**, deux points dans les marges de recette ; continuation inversée refusée ; capture, révision, mesures et calibration inchangées. L'hôte dédié a terminé avec succès. Le serveur utilisateur 8765 n'a pas été utilisé.
- Suite complète réservée au bilan final. Les journaux locaux sont dans `output/dpm11-arcs-vision.log` et `output/dpm11-arcs-mcp/`.
- Compilation debug réussie dans `output/dpm11-arcs-app/`, avec 40 avertissements de code inutilisé. Journal : `output/dpm11-arcs-build.log`. Relecture indépendante statique effectuée, sans défaut bloquant relevé.
