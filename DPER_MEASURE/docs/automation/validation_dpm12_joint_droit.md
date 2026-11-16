# DPM-12 — Diagnostic d’un joint droit visible

`inspect_visible_straight_joint` reçoit un guide du joint et deux guides
transverses ordonnés selon son sens. Les trois guides sont donnés en pixels
natifs, sur la même capture figée. Chaque support exige un profil tous les
2 px au plus. Les profils manquants, concurrents ou écartés par l’ajustement
entraînent un refus.

Les intersections doivent être étayées par des pixels proches. À chaque
extrémité, le bord transverse doit fournir des points observés à au moins
3 px de chaque côté du joint, mesurés perpendiculairement à celui-ci.
Les profils avant et après les intersections vérifient la présence du joint
à l’intérieur et l’absence locale de prolongement à l’extérieur.

La réponse conserve les supports, échantillons, intersections, longueur en
pixels et empreinte de capture. Le domaine testé est une transition de contraste
droite, entre deux bords transverses continus. La géométrie cachée, le contact
physique et la conformité restent indéterminés à partir de ces preuves.

## Vérifications du 16 novembre 2026

- Les 24 tests Rust de vision ont réussi, dont la régression du joint
  visible, masqué, double, absent, à bord transverse unilatéral ou
  échantillonné trop largement. Compilation debug réussie.
- La recette HTTP MCP sur un hôte Windows dédié a retrouvé les intersections
  à moins de 0,2 px de la référence synthétique et une longueur de 143 px.
  Six refus ont été vérifiés, dont la révision périmée. Les états de mesure
  et d’inspection sont restés identiques. L’hôte s’est arrêté normalement.

Depuis la racine `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, exécuter
`cargo test --locked dper_vision::tests::` puis
`& output/dpm12-joint/run-host.ps1`. Le client est
`output\dpm12-joint\recipe.py`. Les images synthétiques, le journal HTTP
et les journaux de l’hôte sont disponibles dans
`O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm12-joint\20261116-011010-782`.
