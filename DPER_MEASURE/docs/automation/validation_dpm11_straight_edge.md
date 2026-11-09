# DPM-11 — extrémités visibles d’un bord droit

`inspect_straight_edge_endpoints` ajuste séparément le bord principal et ses deux bords transverses sur les pixels de la capture figée. Il déduit les deux coins des intersections et restitue leur séparation en pixels. Chaque intersection doit rester à 4 px au plus des observations des deux supports et de l’extrémité correspondante du support principal. Les bords transverses doivent être ordonnés suivant le sens de recherche du bord principal. L’angle entre supports doit atteindre 15° et les deux intersections doivent être séparées d’au moins 8 px.

À chaque extrémité, trois profils vérifient le prolongement vers l’extérieur à 3, 5 et 7 px du coin. Le rayon de recherche est de 2 px, avec le seuil de gradient du bord principal et les deux polarités. Une transition détectée, une ambiguïté ou un profil hors image provoquent un refus. L’absence de transition sur ces profils laisse possible une reprise plus loin ou un contour masqué. La réponse contient des candidats 2D et leurs preuves. Le diagnostic est en lecture seule et le champ `endpoint_extent_verified` des supports reste faux. Les arcs et l’incertitude physique restent à traiter.

## Recette manuelle

1. Fermer les autres instances de DPER_MEASURE pour libérer son serveur MCP. Lancer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm11-endpoints-app\debug\dper_measure.exe`, compilé depuis la branche `codex/DPM-11-straight-edge-endpoints` au commit indiqué dans la PR.
2. Dans **Fichiers → Image…**, ouvrir `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm11-endpoints-mcp\fixtures\bord-borne.png`. Depuis ce chat, demander « Exécute la recette DPM-11 des extrémités sur l’image ouverte ». Le chat peut lire le contexte et exécuter le diagnostic. Le résultat attendu est le segment de (50,5 / 90,5) à (190,5 / 90,5) px et une longueur de 140 px, avec une marge de recette de 0,25 px par point et de 0,35 px sur la longueur.
3. Ouvrir `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm11-endpoints-mcp\fixtures\bord-prolonge.png` et refaire le même essai. Un message doit indiquer qu’un bord se prolonge ou reste ambigu. Pour les deux essais, vérifier que la révision, les mesures et la calibration restent identiques entre le contexte relevé juste après l’ouverture du fichier et celui relevé après l’appel. En cas d’écart, transmettre le nom du PNG, le commit et la réponse MCP.

Pour régénérer les deux PNG, se placer dans `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE` et lancer `python .\scripts\verify_straight_edge_endpoints_mcp.py .\output\dpm11-endpoints-manuel` dans un nouveau dossier de sortie. Le générateur nécessite Pillow.

### Paramètres pour un autre client MCP

Appeler `get_measurement_context` après chaque ouverture. Utiliser `capture_id` et `document_revision` comme `capture_id` et `expected_revision` de `inspect_straight_edge_endpoints`. Voici la valeur de son champ `search` pour les deux PNG.

```json
{
  "edge": {"start": {"x": 53.5, "y": 92}, "end": {"x": 187.5, "y": 92}},
  "start_boundary": {"start": {"x": 49, "y": 93.5}, "end": {"x": 49, "y": 130}},
  "end_boundary": {"start": {"x": 192, "y": 93.5}, "end": {"x": 192, "y": 130}}
}
```

## Résultats automatisés

- `cargo test --locked dper_vision::` a réussi ses 22 tests, dont trois nouveaux. Ils couvrent quatre rotations, l’inversion cohérente du sens de recherche, l’absence ou l’inversion d’une limite, un profil hors image et la continuation avec polarité inversée.
- Transport MCP réel sur serveur dédié. Longueur de 140,0000 px, prolongement refusé et sessions inchangées. Le diagnostic d’intersection de la PR #46 a aussi été rejoué avec succès, y compris ses trois refus. L’hôte de test s’est arrêté avec succès.
