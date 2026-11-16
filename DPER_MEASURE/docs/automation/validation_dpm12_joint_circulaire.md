# DPM-12 — Examiner un joint en arc de cercle

`inspect_visible_circular_joint` examine sur une capture figée un joint
en arc de cercle borné par deux bords transverses droits. Fournir un
guide d’arc dans le sens souhaité, ainsi que les guides du bord de début
et du bord de fin. Toutes les coordonnées et les distances sont en
pixels natifs.

Les trois supports exigent au plus 2 px entre profils et rejettent les
profils concurrents, absents ou écartés de l’ajustement. Chaque bord
transverse doit comporter des points observés à au moins 3 px de
part et d’autre du joint, dans la direction radiale. Les extrémités
sont les intersections du cercle et des bords ajustés. Les profils
intérieurs et extérieurs près de ces intersections vérifient la présence
du contour et l’absence locale de prolongement visible.

La réponse contient les supports et leurs échantillons, les extrémités,
l’ouverture signée, la longueur de l’arc et l’empreinte de la capture.
La longueur est calculée sur le cercle ajusté à partir des pixels.
La reconnaissance d’autres courbes et la reconstruction derrière un
masque relèvent de la suite de DPM-12.

## Vérifications du 16 novembre 2026

- Les 25 tests de vision ont réussi. Le scénario du joint circulaire
  vérifie deux orientations, les deux sens de parcours, les reflets
  simulés par un double bord, le masquage et un bord unilatéral.
- La recette HTTP MCP sur hôte Windows dédié a retrouvé un arc de
  94,2465 px pour un quart de cercle de rayon 60 px. Les extrémités
  sont à moins de 0,25 px de la référence synthétique. L’arc a été
  exercé dans les deux sens, avec cinq refus vérifiés. Les états de
  mesure et d’inspection sont restés identiques après le diagnostic.
  L’hôte s’est arrêté normalement.

Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, utiliser
`cargo test --locked dper_vision::tests::` puis
`& output/dpm12-circular-joint/run-host.ps1`. Le client est
`output\dpm12-circular-joint\recipe.py`. Les images synthétiques, les
journaux et les réponses HTTP sont dans
`O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm12-circular-joint\20261116-011534-193`.
