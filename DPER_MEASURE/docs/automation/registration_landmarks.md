# Repères et résidus du recalage

`register_inspection` calcule une similitude sur des correspondances explicites. Son résultat ne remplace pas l'étalonnage indépendant et ne qualifie pas à lui seul une mesure physique.

Le rapport de recalage, disponible dans `get_inspection_state` sous `inspection.registration`, conserve :

| Champ | Signification |
|---|---|
| `correspondences` | Liste originale des couples `plan`/`image`, dans l'ordre de la requête, y compris les points rejetés. |
| `inlier_indices` | Indices des repères retenus dans cette liste originale, à partir de zéro. |
| `residuals_px` | Un résidu par couple original, dans le même ordre ; distance entre le point image observé et le point plan transformé par le fit final. |
| `threshold_px` | Seuil de sélection demandé, en pixels natifs de l'image. |
| `rms_error_px`, `max_error_px` | Statistiques des seuls repères retenus ; les résidus individuels permettent aussi d'inspecter les points rejetés. |

Pour retrouver un repère retenu d'indice `i`, lire `correspondences[i]` et `residuals_px[i]`. Le système ne renumérote pas la liste après le rejet d'un point.

Les coordonnées `plan` sont celles du repère de dessin fourni à la requête ; dans une vue PDF explicite, ce sont les coordonnées natives de cette vue. `image` et les résidus utilisent les pixels natifs de la photographie. La transformation sert à localiser la recherche et ne fournit aucune calibration physique.

Dans **Inspection → Plan de contrôle et inspection IA → Repères et résidus du recalage**, le panneau consultable affiche les coordonnées, la sélection et les résidus. Sa numérotation visuelle commence à 1 ; les indices JSON commencent à 0. Ce panneau de lecture ne remplace pas les commandes de saisie/recalage prévues dans DPM-22.

Les projets `.dperproj` et les sessions du manifeste de rapport conservent ces données. La nouvelle écriture utilise le schéma **15**, qui évite qu'un exécutable limité au schéma 14 relise puis réenregistre silencieusement un projet en supprimant sa nouvelle trace de recalage.

Les anciens rapports sans ces champs restent lisibles. Leurs listes de correspondances et de résidus sont vides et leur seuil vaut `null` : aucun point d'origine n'est inventé à partir de la transformation. Le panneau signale cette absence et propose de refaire le recalage pour conserver la trace. La conservation des repères ne réactive ni une preuve d'inspection historique ni une association importée non revue.
