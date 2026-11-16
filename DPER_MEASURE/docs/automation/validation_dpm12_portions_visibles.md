# DPM-12 — Portions visibles d’un contour

`inspect_visible_contour_spans` examine de une à huit portions indépendantes
sur une capture figée. Il accepte les segments, les arcs circulaires ouverts
et les courbes quadratiques dans le repère natif de l’image. Le rayon commun
va de 1 à 32 px. Les guides droits et circulaires mesurent entre 8 et 510 px,
avec une ouverture minimale de 60 degrés pour les arcs. Les profils de
recherche sont espacés d’au plus 2 px.

Chaque portion renvoie les preuves observées dans les pixels ou un motif de
refus. Pour les segments et les arcs, le support est ajusté sur les points
détectés. Les profils manquants, concurrents ou rejetés par l’ajustement
suffisent à refuser la portion. Les autres portions sont examinées
indépendamment et leurs résultats restent dans l’ordre des guides.
L’étendue retournée correspond aux échantillons observés. Les preuves
portent sur les portions sondées, avec des intervalles non mesurés entre
elles. La réponse contient l’identifiant, la révision et l’empreinte de la
capture. Le diagnostic s’exécute en lecture seule.

## Domaine du guide quadratique

`{"Quadratic":{"start":{"x":60,"y":91},"end":{"x":180,"y":91},"midpoint_offset_px":21}}`

La corde va de `start` à `end`. La normale est `(-dy, dx)` normalisée.
Le décalage signé `b` du milieu doit valoir entre 1 px et un quart de la
longueur `L` de la corde en valeur absolue. Le profil sur la corde au
paramètre `t` est décalé de `4·b·t·(1−t)` vers cette normale. La borne de
vitesse `hypot(L, 4·b)` doit être comprise entre 8 et 510 px. Le rayon de
recherche doit atteindre au plus la moitié du rayon de courbure minimal
du guide. La marge exigée sur la capture vaut le rayon de recherche plus 2 px.

Chaque profil doit contenir un seul bord exploitable. Le contraste doit
garder son signe, les points doivent avancer le long de la corde et le
décalage normal ne peut varier de plus de 1,5 px entre deux profils.
Le résultat quadratique contient les points détectés dans l’ordre des
profils, avec leur nombre et une étendue d’extrémités non vérifiée.
Le guide ne sert qu’à localiser les profils.

La recette `scripts/verify_quadratic_contour_mcp.py <dossier> --port <port>`
s’exécute sur l’hôte HTTP dédié. Elle vérifie les deux sens de parcours,
une erreur verticale inférieure à 0,25 px face à une référence synthétique,
les refus pour masque, double bord, courbure opposée, fenêtre hors image et
révision périmée, ainsi que la conservation des états applicatifs.

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
et les images. Les essais sont synthétiques. Les courbes hors du domaine
documenté, la reconstruction des portions cachées et la qualification physique
restent hors du domaine livré.
