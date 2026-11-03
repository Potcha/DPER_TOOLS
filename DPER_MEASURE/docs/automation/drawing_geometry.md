# Du PDF à l’inspection d’un contour circulaire

Ce parcours complète [le guide d’inspection](usage.md). Une géométrie du PDF localise la recherche sur la photo ; les pixels observés déterminent ensuite le cercle mesuré. Les vues, symboles et fonctions des lignes ne sont pas reconnus automatiquement. Des indices bornés de contact entre extrémités de tracés, boîtes de texte et cercles peuvent aider à classer les hypothèses.

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
3. Appeler `get_drawing_binding_candidates`. Une attache candidate unique dans la vue peut précéder le classement par proximité. Les cercles compatibles et leurs alternatives restent conservés ; ni le rang ni une attache n'établissent une association certaine. Le nominal ne force aucun rayon.
4. Sélectionner un candidat avec `select_drawing_binding`. La `justification`, de 1 à 2 000 caractères, cite la preuve réellement lue sur le plan.
5. Dans **Inspection → Plan de contrôle et inspection IA**, ouvrir l’association puis cocher **Association cote / géométrie vérifiée sur le plan** après revue. Cette action est distincte de **Interprétation du plan vérifiée**, qui concerne la cote et ses unités.
6. Appeler `register_inspection` avec le `drawing_view_id` retourné. Fournir au moins trois correspondances distinctes et cohérentes, réparties dans la vue : points `plan` en coordonnées PDF, points `image` en pixels natifs. Les repères du plan doivent rester dans les limites de la vue choisie.

Un recalage absent ou établi pour une autre page/vue est refusé par `inspect_bound_feature`. Son échelle localise la recherche ; elle ne remplace jamais la calibration physique. Reprendre les révisions retournées après les mutations et après une intervention dans l’interface.

La sélection MCP ne donne jamais la revue humaine. Une association non revue maintient le blocage `drawing_binding_unreviewed`, même si un aperçu de contour a pu être calculé. Sa revue ne qualifie ni la caméra ni le résultat physique.

### Indices d'attache texte / cercle (DPM-34)

Inclure dans la vue explicite le texte, les contours circulaires possibles **et les tracés de renvoi à examiner**. Un support absent de ce périmètre n'est pas recherché sur le reste de la page. L'application cherche seulement un tracé ouvert, simple, rectiligne, continu et non rempli dont une extrémité touche la boîte du texte et l'autre le contour d'un cercle, dans une marge fixe de **2 points PDF**. Les deux orientations sont testées ; la valeur nominale et l'échelle physique n'interviennent pas.

Chaque indice est consultable dans les preuves du candidat, dans l'interface et la réponse MCP : `endpoint_attachment_hypothesis:` suivi d'un objet JSON donnant identifiant/empreinte/source du support, extrémités, écarts, marge et alternatives compatibles. `requires_review: true` reste explicite. Le même indice fait partie de l'empreinte de l'association ; un changement de support ou de contexte impose un nouveau choix et sa revue.

Une seule attache attribuable à ce texte et à ce cercle dans le périmètre peut améliorer le rang. Plusieurs textes touchés, plusieurs cercles touchés ou des supports concurrents conservent leurs hypothèses sans leur donner cette priorité. En l'absence d'indice exploitable, le classement reste fondé sur la proximité. Les traits pointillés/remplis, courbes aplaties, chemins composés, fermés, dégénérés ou insuffisamment visibles sont exclus. Une extraction incomplète ou un budget dépassé suspend l'étape d'attache plutôt que de produire un classement partiel présenté comme sûr.

Ce détecteur ne lit pas les flèches, ne suit pas les réseaux de traits et ne reconnaît pas une ligne d'attache au sens sémantique d'une norme de dessin. Un trait voisin peut avoir une autre fonction : vérifier le plan avant de sélectionner. Aucun candidat n'est sélectionné, confirmé ou mesuré automatiquement. Le PDF synthétique historique `01_circle_leader.pdf` possède un espace trop important entre le trait et sa cote pour cette règle stricte : l'absence d'indice y est attendue, sans élargissement ad hoc de la marge.

### Renvois répartis en plusieurs tracés (DPM-35)

Un PDF peut découper un renvoi en plusieurs objets vectoriels. Dans la vue explicite, l'application peut désormais proposer une chaîne simple de **2 à 8 supports éligibles**. Chaque joint doit relier deux extrémités à au plus **0,25 point PDF** ; les attaches externes au texte et au cercle gardent la marge de 2 points. Les coordonnées originales sont conservées : aucun point n'est moyenné ou déplacé pour fermer un écart.

L'indice `endpoint_attachment_chain_hypothesis:` donne un objet JSON avec les supports ordonnés du texte vers le cercle, leurs sources/styles/empreintes et points dirigés, puis les joints avec leurs deux coordonnées et écarts exacts. Cette provenance apparaît dans les preuves du candidat UI/MCP et participe à la vérification des sources avant sélection/résolution. Modifier ou retirer un seul support impose un nouveau choix et sa revue.

L'ensemble du composant est examiné avant classement. Aucun préfixe de chaîne n'est choisi dans une branche, un cycle, un contact en T, un croisement interne ou un raccord ayant plusieurs partenaires possibles, même si le croisement est très proche d'un joint. Ces refus sont locaux au composant : une chaîne ou une attache simple indépendante reste exploitable. Les chaînes trop longues sont suspendues ; un budget global dépassé suspend toute l'étape, sans priorité partielle. Les anciennes attaches complètes sur un seul tracé conservent leur format ; lorsqu'elles partagent un composant ambigu, leur provenance peut rester consultable mais elle ne donne aucune priorité. Tous les cercles compatibles restent disponibles à la revue.

Ces chaînes sont des hypothèses de continuité géométrique. Elles ne prouvent ni la fonction normative d'un renvoi ni l'interprétation d'une flèche, d'un datum ou d'une vue. Les réseaux complexes restent hors domaine, les unités et cotes conservent leurs contrôles existants, et aucune mesure photo ou conformité n'est créée par un joint graphique.

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

### Unités documentaires (DPM-36)

`DrawingControlPlan.unit_context` et le contexte de chaque caractéristique distinguent une déclaration absente, une unité documentaire unique et des déclarations contradictoires. Les déclarations conservent leur texte original, leur page et, lorsqu'ils sont disponibles, leur boîte PDF et leurs identifiants de mots. Le panneau « Plan de contrôle et inspection IA » affiche l'état et propose « Sources des déclarations d'unités » pour consulter ces preuves. Le même JSON est disponible via `get_inspection_state` et `analyze_drawing`.

Toutes les unités reconnues après un préfixe pris en charge (`Units`, `Unités`, `Toutes les cotes en`, `All dimensions in`) participent à la décision ; la préposition anglaise `in` n'est pas une déclaration en pouces. Une déclaration contradictoire, sur une ligne ou plusieurs pages, ne fournit aucune unité héritée. Aucune portée par page ou par vue n'est inventée. Les unités inline explicites restent distinctes et utilisables ; des suffixes explicitement incompatibles entre nominal et tolérance, ou entre bornes MIN/MAX, suspendent les limites plutôt que de mélanger les unités.

Une réinterprétation justifiée conserve le conflit source et reste à confirmer séparément. Pour une unité proposée dans un document contradictoire, la conversion reste suspendue avant cette confirmation ; cocher seulement une cote sans lui proposer d'unité ne résout pas l'absence d'unité. Le motif `drawing_unit_context_unresolved` suspend la comparaison, sans empêcher l'observation d'un contour photo correctement calibré. Une confirmation documentaire est persistée comme les autres interprétations du plan ; elle n'est ni une qualification physique ni une preuve photo actuelle. Les blocages d'import, d'association, de qualification et d'incertitude restent indépendants.

Les anciens projets sans ces champs restent lisibles et sont signalés « provenance non conservée ». Ils ne reçoivent pas de déclarations sources inventées : réanalyser le PDF pour les obtenir. Les nouveaux projets sont écrits au schéma 16 : un ancien exécutable doit les refuser plutôt que perdre silencieusement cette provenance à la sauvegarde. Ce lecteur ne reconnaît pas encore les doubles cotations générales, les exceptions locales ni les cartouches sans préfixe reconnu.

Ce parcours traite une **cote rayon/diamètre et un contour circulaire complet réellement visible**. Il ne mesure pas un cercle de jeu entre quatre butées. Les lignes, hachures et polygones extraits restent des supports candidats ; ils ne fournissent pas une méthode de contrôle de longueur ou une relation de cotation automatique.

Les commandes ou transformations vectorielles non prises en charge provoquent une abstention explicite. Les clips et masques non résolus suspendent les objets concernés ; glyphes, images, objets cachés et peintures invisibles ne deviennent pas des cercles de contrôle. Un catalogue partiel ou vide ne prouve pas l’absence d’une caractéristique : consulter les avertissements.

La reconnaissance sémantique générale des vues, fonctions des lignes, flèches, datums et répétitions reste à développer dans DPM-3 et les tickets associés. Les indices d'attache ne remplacent pas cette reconnaissance. Perspective, extrémités réelles, profondeurs et faces cachées demandent d’autres méthodes. Une bonne preuve logicielle de contour n’accorde aucune conformité physique : qualification du montage et incertitude totale du mesurande restent nécessaires.
