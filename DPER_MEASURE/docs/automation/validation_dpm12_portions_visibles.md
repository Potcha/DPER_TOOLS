# DPM-12 — Portions visibles d’un contour

`inspect_visible_contour_spans` examine de une à huit portions indépendantes
sur une capture figée. Les guides sont des segments ou des arcs circulaires
ouverts dans le repère natif de l’image. Le rayon commun va de 1 à 32 px.
Le guide doit mesurer entre 8 et 510 px. L’ouverture minimale d’un arc
est de 60 degrés. Les échantillons sont espacés d’au plus 2 px.

Chaque portion renvoie un support ajusté sur les pixels ou un motif de
refus. Les profils manquants, concurrents ou rejetés par l’ajustement
suffisent à refuser la portion. Les autres portions sont examinées
indépendamment et leurs résultats restent dans l’ordre des guides.
L’étendue retournée correspond aux échantillons observés. Les preuves
portent sur les portions sondées, avec des intervalles non mesurés entre
elles. La réponse contient l’identifiant, la révision et l’empreinte de la
capture. Le diagnostic s’exécute en lecture seule.

## Recette

- Les 26 tests Rust de vision ont réussi. La régression examine une
  silhouette mixte dont un côté est masqué. Elle vérifie le refus local,
  la conservation des segments et de l’arc visibles, et l’écart inférieur
  à 0,2 px par rapport à la référence synthétique.
- La recette HTTP MCP vérifie la silhouette visible, la même silhouette
  masquée et un double bord local. Elle contrôle la correspondance aux
  pixels, un refus local de guide circulaire fermé, quatre requêtes
  invalides et l’égalité des états avant et après les appels.

Depuis `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`, exécuter
`cargo test --locked dper_vision::tests::`. La recette Windows utilise
`output\dpm12-spans\recipe.py` avec l’hôte dédié sur le port 18795.
Les six recettes antérieures de DPM-12 sont également intégrées au lanceur
`output\dpm12-spans-acceptance\run-http.ps1`. Ce dossier reçoit les journaux
et les images. Les essais sont synthétiques. Les courbes non circulaires,
la reconstruction des portions cachées et la qualification physique
restent hors du domaine livré.
