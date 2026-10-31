# Du PDF à l’inspection d’un contour circulaire

Ce parcours complète [le guide d’inspection](usage.md). Une géométrie du PDF localise la recherche sur la photo ; les pixels observés déterminent ensuite le cercle mesuré. Les vues, lignes d’attache et caractéristiques ne sont pas reconnues automatiquement.

## Préparer les sources

Ouvrir ou figer l’image, lire `get_measurement_context`, puis analyser le PDF avec `analyze_drawing`. Reprendre les identifiants réels dans `get_inspection_state`. Vérifier le type, l’unité et la source de la cote ; une correction proposée reste à confirmer dans l’interface. Une calibration physique indépendante reste nécessaire pour obtenir des millimètres.

Les positions du plan sont des **points PDF**, origine en haut à gauche, x vers la droite et y vers le bas. Elles sont distinctes des pixels natifs de la photographie et des millimètres physiques. `source.pdf_bbox` et `page_geometry` décrivent ce repère. Les mots originaux et les éventuelles reconstructions restent consultables dans `spatial_text_groups`, `source.word_ids` et `source.interpretation_text`.

## Les six opérations complémentaires

| Outil | Entrées principales et résultat |
|---|---|
| `analyze_drawing_geometry` | `page`, `expected_revision`. Extrait une page du PDF actif appartenant à l’application et installe son catalogue de géométries candidates. Aucun chemin ou hash client ne remplace cette source. |
| `get_drawing_geometries` | `page`, éventuellement `offset`, `limit`, `kind` ou `geometry_id`. Liste paginée des propositions ; un identifiant exact retourne la primitive avec sa provenance détaillée. |
| `define_drawing_view` | `expected_revision`, `page`, `label`, `feature_ids`, `geometry_ids`, éventuellement `replace_view_id`. Définit un périmètre explicite dans une page et retourne son `view.id`. |
| `get_drawing_binding_candidates` | `feature_id`, `view_id`. Retourne les associations circulaires compatibles, leurs alternatives, rangs, preuves et motifs de suspension. |
| `select_drawing_binding` | `expected_revision`, `feature_id`, `view_id`, `candidate_id`, `justification`. Conserve une hypothèse justifiée, avec `reviewed: false`. |
| `inspect_bound_feature` | `capture_id`, `expected_revision`, `feature_id`, `group`, éventuellement `options` et `commit`. Cherche le contour depuis l’association sélectionnée, sans coordonnées ni champ de revue dans cet appel. |

Le catalogue vectoriel est un **cache de session limité à trois pages**. L’analyse d’une quatrième page retire la plus ancienne ; réanalyser une page absente avant de résoudre ses associations. Les catalogues ne sont pas restaurés depuis le projet JSON.

La liste utilise par défaut 50 entrées et limite chaque réponse à 100. Les filtres `kind` sont `line`, `polyline`, `closed_polygon` et `circle_candidate`. La présence d’un objet dans cette liste ne prouve pas sa fonction physique.

## Choisir, revoir et recaler

1. Analyser la page avec `analyze_drawing_geometry`, puis examiner ses `circle_candidate` avec `get_drawing_geometries`.
2. Retenir les caractéristiques et primitives appartenant à la même vue physique, puis appeler `define_drawing_view`. Ce périmètre est choisi explicitement. Une caractéristique ne peut pas appartenir simultanément à deux vues actives ; utiliser `replace_view_id` pour remplacer sa vue.
3. Appeler `get_drawing_binding_candidates`. La proximité classe les cercles compatibles et conserve leurs alternatives ; elle n’établit pas une association certaine. Le nominal ne force aucun rayon.
4. Sélectionner un candidat avec `select_drawing_binding`. La `justification`, de 1 à 2 000 caractères, cite la preuve réellement lue sur le plan.
5. Dans **Inspection → Plan de contrôle et inspection IA**, ouvrir l’association puis cocher **Association cote / géométrie vérifiée sur le plan** après revue. Cette action est distincte de **Interprétation du plan vérifiée**, qui concerne la cote et ses unités.
6. Appeler `register_inspection` avec le `drawing_view_id` retourné. Fournir au moins trois correspondances distinctes et cohérentes, réparties dans la vue : points `plan` en coordonnées PDF, points `image` en pixels natifs. Les repères du plan doivent rester dans les limites de la vue choisie.

Un recalage absent ou établi pour une autre page/vue est refusé par `inspect_bound_feature`. Son échelle localise la recherche ; elle ne remplace jamais la calibration physique. Reprendre les révisions retournées après les mutations et après une intervention dans l’interface.

La sélection MCP ne donne jamais la revue humaine. Une association non revue maintient le blocage `drawing_binding_unreviewed`, même si un aperçu de contour a pu être calculé. Sa revue ne qualifie ni la caméra ni le résultat physique.

## Inspecter par identifiant

Après revue et recalage, appeler `inspect_bound_feature`, par exemple :

```json
{
  "capture_id": "capture-exemple",
  "expected_revision": 25,
  "feature_id": "identifiant-cote-retourne",
  "group": "Groupe 1 · contour circulaire visible",
  "options": null,
  "commit": false
}
```

Les identifiants et la révision sont à remplacer par ceux de la session. Aucun `geometry_in_drawing` n’est fourni : l’application résout la primitive associée. Vérifier les points acceptés et rejetés, les résidus, la couverture et les blocages dans `record` et `get_inspection_state`.

Un aperçu conserve un enregistrement et incrémente la révision. Pour engager la mesure, reprendre cette révision et rappeler l’outil avec `commit: true` : les contours sont détectés de nouveau. Le cercle natif conserve le centre et le rayon du fit robuste ; les échantillons observés restent séparés dans la preuve.

L’ancien outil `inspect_feature` avec coordonnées libres est refusé pour une caractéristique possédant une association. Utiliser `inspect_bound_feature` pour conserver cette provenance. Le parcours libre reste disponible pour les caractéristiques sans association.

## Sources modifiées et projets rechargés

L’application compare l’empreinte au contenu du fichier réellement présent. L’extraction vectorielle contrôle ce contenu avant et après son travail ; un PDF modifié pendant l’extraction est refusé. Les associations lient aussi la caractéristique, la primitive et leur vue. Un chemin identique ou un hash copié dans du JSON ne démontre pas que la source est actuelle.

Après modification du PDF, réanalyser le plan et les géométries, redéfinir les vues et refaire les associations et recalages nécessaires. Remplacer une vue retire ses associations et invalide le recalage qui en dépend. Les anciennes mesures natives restent conservées et ne deviennent pas automatiquement des observations du nouveau plan.

Au rechargement, `BindingSelection.reviewed` revient à `false` et les preuves d’inspection à `imported: true`, quelles que soient les valeurs du JSON. Réanalyser les catalogues dans la session courante, revoir les associations dans l’interface et refaire l’inspection des pixels avant de se fonder sur une preuve actuelle.

Un snapshot peut avoir subi un arrondi numérique lors du chargement JSON. Si son empreinte ne correspond plus exactement au catalogue frais, renouveler la sélection avec `select_drawing_binding` avant la revue ; aucune tolérance d'égalité des sources ne contourne ce garde-fou.

## Domaine et abstention

Ce parcours traite une **cote rayon/diamètre et un contour circulaire complet réellement visible**. Il ne mesure pas un cercle de jeu entre quatre butées. Les lignes, hachures et polygones extraits restent des supports candidats ; ils ne fournissent pas une méthode de contrôle de longueur ou une relation de cotation automatique.

Les commandes ou transformations vectorielles non prises en charge provoquent une abstention explicite. Les clips et masques non résolus suspendent les objets concernés ; glyphes, images, objets cachés et peintures invisibles ne deviennent pas des cercles de contrôle. Un catalogue partiel ou vide ne prouve pas l’absence d’une caractéristique : consulter les avertissements.

La reconnaissance sémantique générale des vues, lignes d’attache, datums et répétitions reste à développer. Perspective, extrémités réelles, profondeurs et faces cachées demandent d’autres méthodes. Une bonne preuve logicielle de contour n’accorde aucune conformité physique : qualification du montage et incertitude totale du mesurande restent nécessaires.
