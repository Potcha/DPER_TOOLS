# DPM-12 — Observer les contours de pièces séparées

`inspect_separated_polygon_contours` examine de deux à quatre pièces sur
une même capture figée. Chaque guide comporte de trois à huit sommets dans
l’ordre d’un polygone simple, convexe ou concave, en pixels natifs. Le rayon
commun est compris entre 1 et 32 px. La distance minimale entre les côtés
des guides doit dépasser deux fois le rayon plus 2 px. Pour un rayon de
4 px, le seuil est donc de 10 px. Les sommets reconstruits doivent chacun
rester à une distance de leur guide inférieure ou égale au rayon.

Le résultat suit l’ordre des guides. Chaque pièce observée fournit les
supports ajustés sur les pixels, leurs échantillons et les intersections
des côtés. Une pièce masquée ou absente renvoie un refus local et son motif.
Les guides en contact, croisés, emboîtés ou trop proches entraînent un refus
de la requête.
La réponse contient l’identifiant, la révision et l’empreinte de la capture.
Le diagnostic est en lecture seule et s’exprime en pixels.

Ce périmètre couvre les silhouettes droites séparées qui restent visibles.
Les pièces jointives ou superposées et les contours courbes relèvent de la
suite de DPM-12. Les guides localisent la recherche et le résultat provient
des contours observés.

## Première recette du 16 novembre 2026

Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE` :

```powershell
cargo test --locked dper_vision::tests::
& output/dpm12-separated/run-host.ps1
```

- Les 22 tests de vision ont réussi. La nouvelle régression vérifie deux
  contours, la conservation d’un contour quand l’autre est absent, l’écart
  au guide décalé et le refus de régions superposées.
- La recette HTTP MCP sur un hôte Windows dédié sans fenêtre a observé deux
  rectangles synthétiques avec des guides décalés de 0,75 px et des sommets
  à moins de 0,2 px de la référence. Une pièce absente ou masquée a été
  refusée sans perdre la preuve de l’autre.
- Les guides superposés, la révision périmée, le guide unique et le rayon
  hors limites ont été refusés. Les états d’inspection et de mesure sont
  restés identiques. L’hôte de test s’est arrêté normalement.

Le client est `output\dpm12-separated\recipe.py`. Les images, journaux et
réponses HTTP sont dans
`O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm12-separated\20261116-010016-876`.
Le diagnostic s’exerce aussi depuis un client MCP connecté à
`O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`.

## Recette des pièces proches du 16 novembre 2026

La distance entre les côtés remplace le contrôle initial des rectangles
englobants élargis. Deux rectangles distants de 15 px sont observables avec
un rayon de 4 px. Les guides sont décalés de 0,75 px et les sommets observés
restent à moins de 0,2 px de la référence synthétique. Une pièce absente ou
masquée laisse la preuve de l’autre disponible.

Les 23 tests Rust de vision ont réussi. La recette HTTP MCP sur l’hôte
Windows dédié, port 18792, a vérifié les trois scènes et huit refus de
requête, dont le contact, le croisement, l’emboîtement et la distance
exactement égale au seuil. Les états de mesure et d’inspection sont restés
identiques après les appels. L’hôte a été arrêté normalement.

Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, rejouer avec
`& output/dpm12-close/run-host.ps1`. Le client est
`output\dpm12-close\recipe.py` et les entrées, journaux et réponses sont dans
`O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm12-close\20261116-010452-057`.
