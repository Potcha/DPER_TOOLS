# Utiliser l’incrément 1 d’inspection assistée

La recherche dirigée s'abstient désormais lorsque plusieurs contours sont aussi plausibles. Les profils ambigus sont exclus de l'ajustement ; une couverture restante insuffisante refuse l'inspection sans créer de mesure. Voir [le corpus DPM-27 et ses limites](validation_dpm27.md), notamment le rôle de la polarité, de la localisation et des reflets.

La lecture PDF dispose désormais de groupes textuels localisés : `page_geometry`, `spatial_text_groups` et `source.pdf_bbox` sont en points PDF. Les mots originaux, `source.word_ids` et l'éventuel `source.interpretation_text` rendent une reconstruction inspectable. Ils ne décrivent pas des contours mesurés sur la photographie. Voir [la validation de cette livraison](validation_dpm18.md) et `scripts/run_inspection_smoke.ps1` pour un essai séparé de la session utilisateur.

Pour choisir une primitive du PDF puis inspecter une caractéristique **par identifiant, sans ressaisir sa géométrie**, suivre [le parcours des géométries et associations](drawing_geometry.md). Ses six outils complètent ce guide : catalogue de page, vue explicite, alternatives d’association, sélection justifiée et `inspect_bound_feature`. La revue de l’association appartient uniquement à l’interface ; une sélection MCP ne l’accorde pas.

Pour relier explicitement les mesures dérivées à leurs parents, consulter le [contrat des dépendances de mesures](dependencies.md).

Ce parcours décrit le contrat présent dans les sources. Il ne prouve pas son exécution dans l’application déjà ouverte. Un nouvel exécutable doit être lancé ou l’application relancée, puis le catalogue MCP reconnecté si nécessaire, avant d’utiliser les nouveaux outils. Les exemples sont illustratifs : remplacer chemins, identifiants, coordonnées et révisions par les valeurs réellement lues dans la session.

L’incrément analyse le texte du plan, recale des correspondances fournies explicitement et cherche localement des contours. Il ne reconnaît pas automatiquement une pièce et toutes ses caractéristiques depuis un PDF et une photographie.

## 1. Préparer la capture

Ouvrir l’image ou figer la caméra avec les outils existants, puis lire `get_measurement_context`. Conserver `capture_id` et `document_revision`. Les coordonnées des mesures natives ont leur origine en haut à gauche, x vers la droite et y vers le bas.

La capture doit être éditable, avec une image figée et aucun tracé ou autre geste en cours. Vérifier les critères de qualité dans l’UI. Si une valeur physique est recherchée, établir la calibration avec une référence indépendante de la cote à contrôler. L’échelle du recalage plan/photo ne vaut pas calibration.

Les cinq nouveaux outils sont `analyze_drawing`, `propose_drawing_interpretation`, `get_inspection_state`, `register_inspection` et `inspect_feature`. Leurs mutations demandent une révision entière obligatoire. Après chaque mutation, reprendre la révision retournée ; une révision périmée provoque un refus. L'extraction PDF s'exécute hors de la boucle d'interface ; si le document change pendant l'analyse, l'installation du plan est refusée.

## 2. Analyser le plan

Après extraction, une unité ou un type indéterminé peut être corrigé dans le panneau du plan. L'IA peut aussi appeler `propose_drawing_interpretation` avec `expected_revision`, `feature_id`, `kind` (par exemple `diameter`), `unit` (par exemple `millimeter`) et `source_note` citant la preuve lue sur le plan. La source, le nominal et les tolérances restent conservés ; la proposition est journalisée, non confirmée, et le recalage est invalidé. L'opérateur peut ensuite cocher « Interprétation du plan vérifiée » dans l'interface. Une interprétation confirmée ne valide aucune mesure physique.

Arguments de `analyze_drawing` :

```json
{
  "path": "O:\\Mesures\\plan_piece.pdf",
  "expected_revision": 12
}
```

Le chemin doit désigner un PDF ou une image locale pris en charge par l’extraction existante. Le parseur conserve les textes et provenances disponibles, extrait les unités et tolérances explicites et signale les ambiguïtés. Un texte OCR n’est pas une exigence confirmée automatiquement.

Cette opération remplace le plan d’inspection actif, efface son recalage et ses enregistrements et retire l’ancien document de comparaison associé. Elle n’efface pas les mesures natives déjà dessinées. Vérifier les anciennes mesures avant de les réutiliser avec le nouveau plan.

Lire ensuite `get_inspection_state` sans arguments :

```json
{}
```

La réponse contient `capture_id`, `document_revision`, `inspection` et `limitations`. Dans `inspection.plan.characteristics`, reprendre l’identifiant réel de la caractéristique, son type, son unité, sa tolérance, son texte source et les motifs de limitation. Les identifiants comme `drawing-1` ci-dessous sont des exemples.

Le panneau permet de consulter et de corriger le type ou l’unité de ces candidats, puis de cocher **Interprétation du plan vérifiée** après revue du document source. `propose_drawing_interpretation` enregistre une proposition justifiée sans accorder cette confirmation humaine. Chaque correction du type ou de l’unité retire la confirmation et invalide le recalage ; relire la révision puis refaire le recalage avant l’inspection. La confirmation d’une interprétation ne valide ni le contour ni la conformité physique.

## 3. Fournir les correspondances plan/photo

Les coordonnées d'origine, les résidus de tous les repères et le seuil de sélection sont conservés dans le recalage, les projets et les manifestes de rapport. Voir [la traçabilité des repères](registration_landmarks.md) pour les indices retenus, les points rejetés et les anciens projets qui ne possèdent pas ces données.

Arguments de `register_inspection` :

```json
{
  "capture_id": "capture-exemple",
  "expected_revision": 13,
  "correspondences": [
    {"plan": {"x": 0.0, "y": 0.0}, "image": {"x": 100.0, "y": 100.0}},
    {"plan": {"x": 50.0, "y": 0.0}, "image": {"x": 600.0, "y": 100.0}},
    {"plan": {"x": 0.0, "y": 50.0}, "image": {"x": 100.0, "y": 600.0}}
  ],
  "maximum_error_px": 1.5
}
```

Ces coordonnées illustrent une image d’au moins 601 × 601 pixels. Choisir de vrais repères correspondants, distincts et répartis dans la zone étudiée. `plan` utilise un repère de dessin cohérent choisi pour le recalage ; `image` utilise les pixels natifs de la capture. La géométrie de recherche ultérieure devra employer exactement le même repère de dessin. Les coordonnées du dessin ne sont pas automatiquement reconstruites depuis la disposition du PDF.

Pour une caractéristique associée à une primitive extraite, fournir aussi `drawing_view_id` dans `register_inspection`. Les points `plan` sont alors les points PDF natifs de cette vue et doivent rester dans ses limites. Un recalage pour une autre page/vue ne permet pas l’inspection de cette association. Le catalogue de session, limité à trois pages, doit être disponible et correspondre au fichier actuel.

Le point d’entrée MCP accepte au maximum 64 correspondances. Le calcul nécessite au moins trois repères distincts et cohérents et un support suffisant ; il estime translation, rotation et échelle avec rejet des correspondances aberrantes. La réponse conserve les indices retenus ainsi que les résidus RMS et maximal en pixels. Les modèles projectifs, réflexions et échelles anisotropes ne sont pas pris en charge.

Un nouveau recalage efface les enregistrements d’inspection précédents, sans supprimer leurs anciennes mesures natives. Reprendre la nouvelle révision avant toute inspection.

## 4. Prévisualiser une caractéristique

Arguments de `inspect_feature` pour un cercle :

Cet exemple à coordonnées libres concerne une caractéristique **sans association au PDF**. Si elle possède une association, ce point d’entrée est refusé : appeler `inspect_bound_feature` avec son `feature_id`, la capture, le groupe et la révision courante, sans `geometry_in_drawing`. Voir le [guide associé](drawing_geometry.md) pour le choix et la revue de cette association.

```json
{
  "capture_id": "capture-exemple",
  "expected_revision": 14,
  "feature_id": "drawing-1",
  "group": "Groupe 1 · alésage visible",
  "geometry_in_drawing": {
    "Circle": {
      "center": {"x": 25.0, "y": 25.0},
      "radius": 5.0
    }
  },
  "options": null,
  "commit": false
}
```

`group` est obligatoire et non vide. `geometry_in_drawing` localise les profils de recherche ; elle ne fournit pas une valeur mesurée. `feature_id` doit désigner une caractéristique compatible : un cercle ou un arc pour un rayon ou diamètre ; une ligne pour une caractéristique linéaire. Un contrôle d’angle entre deux supports demande une méthode dédiée encore absente.

`InspectionGeometry` utilise la représentation Serde à étiquette externe : respecter exactement les noms `Circle`, `Line` ou `Arc`, avec leur majuscule. Ne pas remplacer cette structure par un champ `kind`.

Autres géométries valides pour le champ `geometry_in_drawing` :

```json
{"Line": {"start": {"x": 10.0, "y": 20.0}, "end": {"x": 40.0, "y": 20.0}}}
```

```json
{
  "Arc": {
    "center": {"x": 25.0, "y": 25.0},
    "radius": 5.0,
    "start_rad": 0.0,
    "sweep_rad": 1.5707963267948966
  }
}
```

Les angles d’arc sont en radians. Les rayons et points de ces trois propositions sont dans le repère du dessin enregistré. Les points retournés dans la preuve sont dans les pixels natifs de la photographie.

Avec `options: null` ou sans `options`, les valeurs par défaut sont utilisées. Pour les modifier, fournir l’objet complet typé :

```json
{
  "sample_count": 96,
  "search_radius_px": 8.0,
  "minimum_gradient": 12.0,
  "polarity": "Any",
  "minimum_coverage": 0.7,
  "outlier_threshold_px": 1.5,
  "maximum_rms_px": 0.8,
  "minimum_arc_sweep_rad": 1.0471975511965976
}
```

`polarity` vaut exactement `Any`, `Rising` ou `Falling`. Le rayon de recherche et les résidus sont en pixels. Les limites de travail sont bornées par le moteur ; un contour absent, un support insuffisant ou des résidus excessifs provoquent un refus. Ces réglages de vision restent distincts des seuils de validation et d’un budget d’incertitude physique.

**Un aperçu est enregistré dans l’historique d’inspection.** `commit: false` n’ajoute aucune mesure native mais conserve un `record`, marque la session comme modifiée et incrémente `document_revision`. La réponse contient `record` et la nouvelle révision. Un nouvel aperçu ou un engagement nécessite cette nouvelle révision.

Consulter les points acceptés et rejetés, `record.result.evidence`, `value_mm`, `outcome` et ses `blocking_reasons`. Un aperçu sans mesure native associée peut être signalé `measurement_missing` lors de sa réévaluation par `get_inspection_state` ; il ne constitue pas une mesure engagée.

## 5. Engager les observations et revoir dans l’UI

Après revue de l’aperçu, répéter `inspect_feature` avec les mêmes paramètres adaptés au contexte actuel, `expected_revision` actualisée et `commit: true`. La détection est recalculée ; l’outil ne transforme pas simplement un ancien aperçu en mesure.

Pour un cercle ou un arc accepté, l’application crée une mesure native `Circle` avec le centre et le rayon issus de l’ajustement robuste, avec origine `vision`, et rattache son identifiant au nouvel enregistrement. Les points du contour restent conservés séparément dans la preuve ; le résultat robuste n’est pas remplacé par un second ajustement natif. L’historique d’inspection conserve au maximum 1 000 enregistrements ; retirer le plus ancien de cet historique ne supprime pas sa mesure native. L’arc permet ici un contrôle de rayon ou diamètre depuis son support ; il ne démontre pas ses extrémités ni sa longueur réelle. Pour un rayon, lire la valeur et le type de caractéristique dans l’enregistrement d’inspection : l’outil natif `Circle` présente par ailleurs un diamètre.

L’engagement d’une ligne est actuellement refusé : le support de droite est ajusté, mais ses extrémités réelles ne sont pas détectées. L’aperçu ne suffit donc pas à établir la longueur de la pièce. L’ouverture réelle et les extrémités d’un arc ne sont pas davantage mesurées.

Dans l’onglet Inspection, ouvrir **Plan de contrôle et inspection IA**. Consulter **Preuves et limites**, vérifier le groupe, la caractéristique, la couverture et les points superposés à l’image. **Sélectionner M… pour correction** sélectionne une mesure native engagée ; les outils de correction manuelle existants permettent ensuite de l’ajuster. Une correction manuelle ne recrée pas automatiquement la preuve du détecteur : relancer l’inspection après une modification pertinente.

`get_inspection_state` réévalue les preuves contre les pixels, la capture, la calibration, le recalage et la mesure actuellement liée. Une modification incompatible suspend leur état. Les motifs incluent notamment `source_pixels_changed`, `registration_missing_or_stale`, `measurement_geometry_changed` et `calibration_changed`. Refaire le recalage ou l’observation nécessaire, puis relire l’état ; ne pas réutiliser un statut sauvegardé comme preuve actuelle. La propagation des dépendances de mesures est décrite dans le [contrat dédié](dependencies.md).

## 6. Ajouter un repère de construction par MCP

Le champ `is_construction` est disponible dans chaque élément de `add_measurements`. Exemple dans le repère natif de la photographie :

```json
{
  "capture_id": "capture-exemple",
  "expected_revision": 16,
  "measurements": [
    {
      "kind": "circle",
      "points": [{"x": 350.0, "y": 350.0}, {"x": 400.0, "y": 350.0}],
      "label": "Repère de recherche",
      "method": "Construction proposée ; contour non mesuré",
      "is_construction": true
    }
  ]
}
```

Pour `kind: "circle"`, les deux points représentent centre puis bord. Une construction est exclue des données de mesure et de conformité ; ses composantes de mesure exposées sont vides. Omettre `is_construction` conserve le comportement de mesure native ordinaire, sans créer pour autant une preuve de vision ou une validation physique.

## 7. Sauvegarder et exporter avec les limites

La session `.dperproj` utilise le schéma **15** et conserve `automated_inspection` : plan, recalage, groupes, observations et états historiques. Les anciens projets sans ce champ restent lisibles. Au rechargement, chaque enregistrement désérialisé est considéré `imported: true`, même si le fichier affirmait `false`. L’état historique est conservé pour la traçabilité ; une nouvelle lecture métier ne lui accorde pas une validation actuelle sans nouvelle inspection des pixels. Les dépendances de mesures sont restaurées selon le [contrat dédié](dependencies.md) ; les versions de schéma futures sont refusées.

Les associations cote–géométrie sauvegardées reviennent également à `reviewed: false` ; aucun champ JSON ne remplace la revue UI de la session actuelle. Réanalyser les catalogues vectoriels, revoir les associations et refaire l’observation nécessaire. Le fichier PDF réel est contrôlé par son contenu, notamment avant et après l’extraction ; des sources modifiées suspendent leur utilisation.

Pour le PDF, appeler `prepare_report`, examiner les éléments et sélectionner les clés exactes des images qui portent les mesures à inclure. Puis appeler `export_report`, par exemple :

```json
{
  "capture_id": "capture-exemple",
  "expected_revision": 17,
  "format": "pdf",
  "destination": "O:\\Mesures\\rapport_inspection.pdf",
  "selected_image_keys": ["capture_piece.png"],
  "title": "Relevé exploratoire de la pièce",
  "audit_notes": "Contours revus sur la capture ; limites de méthode et qualification physique à examiner.",
  "approval_status": "À revoir",
  "include_audit_page": true,
  "overwrite": false
}
```

La clé d’image et la révision de cet exemple doivent être remplacées par celles de `prepare_report`. L’export MCP ajoute aux notes d’audit un état réévalué des inspections : groupe, caractéristique, mesure native, valeur, validation logicielle, conformité et blocages. Les générateurs communs PDF/PPTX ajoutent également une mention obligatoire par photo concernée, y compris depuis les exports manuels et lorsque la fiche d’audit est désactivée. Les états sauvegardés y sont explicitement historiques et non réévalués ; les verdicts des mesures d’origine `vision` restent suspendus. Les exports CSV/XLSX signalent aussi cette suspension. La comparaison historique des autres mesures reste inchangée.

Le manifeste compagnon `.dper_measure.json` conserve les données de session, dont l’inspection et les preuves. Vérifier les avertissements et distinguer les notes réévaluées par le parcours MCP des états historiques conservés dans la session. Aucun de ces exports ne transforme une preuve sauvegardée en qualification physique actuelle.

## Parcours natif disponible — DPM-22 / UI-01

Après l'import du plan PDF, le panneau **Géométries proposées du PDF** permet de choisir une page reconnue et de lancer son analyse vectorielle. Le traitement travaille en arrière-plan ; une seule analyse native peut être active à la fois. Un changement de révision du document, une action de mesure en cours ou une modification du fichier source empêche l'installation du résultat. Le bouton permet de réessayer après correction de l'erreur.

Le catalogue expose les avertissements d'extraction, un filtre de famille, vingt propositions par page de liste et le détail de chaque primitive : boîte en points PDF, cercle proposé éventuel, élément source, attributs et transformations. L'analyse, les contrôles d'installation et la consultation utilisent les mêmes opérations que le MCP. Les textes et images ignorés ainsi que les tracés non pris en charge sont comptabilisés ; un PDF scanné peut ne proposer aucun vecteur.

Les catalogues sont temporaires : trois pages au maximum restent en mémoire. Après éviction ou rechargement du projet, réanalyser la page souhaitée. Cette consultation ne crée aucune mesure et ne confirme aucune association.

Le panneau **Recalage par repères explicites** permet ensuite une saisie experte manuelle sur une image figée. **Nouveau brouillon** prépare trois lignes vides pour la capture, la révision et le PDF source actuels ; il remplace toute saisie précédente. Chaque ligne associe un point du PDF, exprimé en points PDF avec origine en haut à gauche, à un point réellement identifié dans l'image, exprimé en pixels natifs avec la même origine. Le zoom d'affichage et les millimètres ne sont pas ces coordonnées. Ajouter ou retirer des lignes permet de renseigner de trois à soixante-quatre repères distincts. Aucun repère n'est automatiquement déduit des coins, d'un nominal ou d'une calibration.

Sur chaque ligne, **Choisir sur la photo** arme uniquement ses coordonnées image. Cliquer puis relâcher sur un point réellement identifié de la photo renseigne **Image X** et **Image Y** dans les pixels natifs, en tenant compte du zoom, du déplacement et de la densité de l'écran. Les coordonnées du PDF restent saisies manuellement. La molette et le bouton du milieu permettent de naviguer pendant le choix ; un glissement, un clic hors de la photo ou sur sa limite droite/basse ne valide aucun point. Un geste de navigation ne choisit pas de repère.

**Annuler le choix sur la photo**, **Échap** ou `cancel_active_action` conservent les anciennes coordonnées. Le relâchement d'un clic déjà commencé reste réservé au choix annulé et ne crée aucune mesure ou annotation. Choisir une autre ligne réarme explicitement cette ligne ; retirer une ligne annule le choix afin qu'un indice déplacé ne reçoive aucun point. Pendant le choix et jusqu'au relâchement d'un geste annulé, les mutations concurrentes sont bloquées. Un changement de capture, de révision, du PDF source ou des pixels empêche l'acceptation du point ; préparer de nouveau le brouillon si son contexte a changé. Ce choix prépare seulement une correspondance : il n'applique pas le recalage, ne crée aucune mesure et ne change jamais l'étalonnage.

Renseigner le seuil de résidu en pixels natifs, choisir éventuellement une vue PDF déjà définie, puis **Calculer et appliquer la similitude**. L'éditeur appelle la même opération `Register` que le MCP : mêmes contrôles des correspondances, du support, des résidus et des limites d'une vue explicite. Le résultat indique les repères retenus et rejetés ainsi que leurs résidus dans le panneau du plan de contrôle. La transformation localise une recherche par translation, rotation et échelle ; son échelle ne modifie jamais l'étalonnage physique. Appliquer un nouveau recalage invalide les observations antérieures.

Le brouillon conserve son contexte de préparation. Si la capture, la révision, les pixels ou le PDF source changent, il ne peut pas être appliqué : préparer un nouveau brouillon ou reprendre explicitement les repères du recalage courant, puis les revoir. La saisie périmée reste consultable et n'est jamais silencieusement appliquée à une autre image. La reprise exige des correspondances et un seuil conservés compatibles avec la capture actuelle ; elle ne transforme pas un recalage historique incomplet en preuve actuelle. L'éditeur ne fige pas lui-même la caméra et demande de terminer les tracés, éditions, étalonnages ou exports en cours.

Le panneau **Vues et associations du plan PDF** permet de préparer une vue sur une page analysée, nommer cette vue et cocher explicitement ses caractéristiques et primitives. **Enregistrer la vue explicite** appelle la même opération DefineView que le MCP. Pour modifier une vue existante, choisir son remplacement puis préparer/recharger le brouillon ; ses associations et son recalage liés seront invalidés. Les catalogues évincés doivent être réanalysés. Les brouillons conservés deviennent inutilisables après une modification de contexte et ne sont jamais appliqués silencieusement à une nouvelle page/capture/source.

Choisir ensuite la vue et sa caractéristique, puis **Préparer / recharger les associations**. Comparer les cercles proposés, sélectionner explicitement un candidat et écrire sa justification avant **Enregistrer l'association choisie**. Le classement par proximité ne choisit rien à votre place. Cette opération Select laisse l'association non revue ; la case de revue dans le plan de contrôle est une action distincte, après vérification sur le plan source. Les associations automatiques actuelles ne concernent que les cercles complets compatibles avec un diamètre/rayon. Les autres recherches restent explicites ; aucun algorithme général d'appariement n'est revendiqué.

Après le recalage explicite, ouvrir **Observer les contours de la photo** : choisir la caractéristique et un **Groupe de contrôle**, puis préparer une observation. Une association PDF impose sa géométrie et sa vue de recalage ; aucune substitution manuelle n'est possible. Sans association, renseigner une localisation de droite, cercle ou arc dans le repère du dessin. Les valeurs d'exemple doivent être remplacées par les repères réellement identifiés. Les réglages bornés de recherche (rayon en pixels natifs, profils, polarité, couverture et résidus) sont modifiables avant observation.

**Prévisualiser les contours (sans mesure)** appelle l'opération partagée Inspect/InspectBound et conserve une trace d'aperçu, sans créer de mesure native. Vérifier les points réellement détectés, le rayon/support ajusté, la couverture et les résidus. L'enregistrement demande ensuite un clic distinct sur **Enregistrer explicitement l'observation** ; une modification du groupe, des réglages ou du contexte impose une nouvelle préparation/prévisualisation. La mesure enregistrée conserve le groupe et les preuves. Une seconde action ne duplique pas automatiquement l'enregistrement. Les droites restent des supports observés : l'enregistrement d'une longueur est refusé tant que les extrémités réelles ne sont pas mesurées. Un arc permet le rayon, pas la longueur physique entre ses extrémités.

**Réévaluer les observations existantes** utilise GetState et les règles communes d'évaluation. Les résultats affichés sont réévalués contre le contexte courant ; les pixels/source modifiés suspendent les anciens résultats. Le plan de contrôle expose les groupes, mesures et blocages, ainsi qu'un bouton de sélection de la mesure native pour correction manuelle. Une correction invalide les preuves et dépendances correspondantes ; réobserver avant toute utilisation comme preuve courante. Sauvegarder le projet et exporter le rapport utilisent les fonctions existantes ; une preuve rechargée reste historique et les rapports conservent les limites.

Ce parcours termine le raccordement assisté de première phase demandé par DPM-22. Les coordonnées côté PDF peuvent rester saisies manuellement. Les extrémités de lignes et d'arcs ne sont pas mesurées, et aucune calibration ni conformité physique ne découle des vecteurs du PDF ou du recalage. L'appariement universel, les modèles de jeu tangent à plusieurs butées et la qualification physique appartiennent aux autres travaux du backlog.

## États, groupes et preuves — DPM-23

Chaque objet natif indique sa nature : **Construction · hors mesure**, **Proposée · à observer ou réévaluer**, **Mesurée · à vérifier**, ou **Vérifiée par logiciel**. L'origine et le nom d'une méthode ne suffisent jamais à obtenir un état validé. Le groupe et les sources de dépendance sont visibles dans la fiche. Une preuve périmée ou importée suspend la valeur courante ; consulter les motifs puis refaire l'observation. La construction reste exclue des cotes et de la conformité.

Dans le plan de contrôle, les observations sont regroupées par leur groupe de contrôle ; plusieurs observations d'une même caractéristique et les aperçus restent distincts. Ouvrir **Preuves, seuils et limites** ou **Preuves et limites · M…** pour consulter les points acceptés/rejetés, la couverture, les résidus, la provenance, les réglages réellement utilisés et les seuils actuels de réévaluation logicielle. Les réglages de recherche sont conservés avec l'observation et le projet ; un ancien historique sans ces réglages affiche explicitement qu'ils sont inconnus. Aucun réglage par défaut n'est attribué rétroactivement.

Les motifs de péremption et d'impossibilité 2D restent visibles même avec les détails fermés. Le bouton de sélection pour correction garde le lien avec la mesure native. Une observation d'arc ou de support de droite ne démontre pas la position de ses extrémités réelles.

Les rapports PDF/PPTX conservent les groupes, la nature des objets, les preuves, les réglages connus, les dépendances et les motifs, même si la page facultative d'audit est désactivée. Ces annexes sont explicitement **historiques et non réévaluées** ; elles n'accordent pas une conformité actuelle à un état sauvegardé. Les CSV/XLSX ajoutent les mêmes métadonnées historiques aux lignes de mesure disponibles, en conservant l'exclusion des constructions et des valeurs périmées. Le manifeste conserve le détail complet. Les notes, identifiants longs et données denses sont répartis sur plusieurs lignes/pages sans tronquer les preuves.

## Limites actuelles à conserver dans le compte rendu

- Le PDF est interprété conservativement à partir du texte disponible ; l’association sémantique complète entre vues, lignes d’attache et caractéristiques n’est pas accomplie.
- Les correspondances plan/photo et les propositions locales restent explicites ; aucun appariement automatique universel n’est fourni.
- Le service d’inspection ne dispose pas encore d’une qualification physique ni de l’incertitude totale par mesurande : il force ces informations à « non qualifiée » et « inconnue ». Il ne doit donc pas annoncer une conformité physique, même avec un contour logiciellement validé.
- Une U fournie pour la calibration ne remplace pas le budget de la mesure. Une incertitude en millimètres ne devient pas une incertitude en degrés.
- Un cercle de jeu tangent à **quatre butées** demande un modèle et des observations dédiés ; l’ajustement d’un contour circulaire ne l’implémente pas.
- Les profondeurs, faces cachées, dimensions hors plan et perspectives non justifiées restent impossibles à valider depuis cette seule méthode 2D.

Les essais effectivement réalisés dans l’application relancée, les tests logiciels et les essais physiques doivent faire l’objet de comptes rendus séparés. Ce guide n’annonce aucun de ces contrôles comme exécuté.


## Base documentaire de décision de conformité (DPM-16)

Après avoir enregistré une observation de contour et confirmé son exigence et son association au plan, ouvrir **Base documentaire de décision de conformité**. Choisir l'observation du bon groupe et **Préparer la revue de cette observation**. Ce panneau utilise un plan PDF source disponible et une image figée.

Renseigner la référence du document de qualification indépendante, son domaine d'application à cette caractéristique et à ce montage, la **U totale élargie du résultat mesuré en mm**, sa source documentaire et le responsable de la revue. Le bouton d'attestation applique la base à cette observation précise. Cette saisie consigne une qualification externe déjà établie et revue par l'opérateur ; elle ne constitue ni une qualification du matériel par le logiciel, ni une nouvelle propagation d'incertitude. Une U de référence ou du profil seul, la mention historique « calibration vérifiée », le nominal et l'échelle de recalage ne remplacent pas cette base. Pour un rayon, fournir la U totale de ce rayon, et non celle d'un diamètre.

La règle conservatrice est indépendante de la validation du contour :

- **Conforme** si tout l'intervalle `[valeur − U ; valeur + U]` appartient aux limites confirmées, bornes incluses.
- **Hors tolérance** si l'intervalle entier est strictement disjoint des limites.
- **Indéterminée** en cas de chevauchement, contact extérieur avec une limite, qualification/U absentes, invalides ou périmées, ou preuve de contour incompatible.
- **Non évaluée** sans tolérance confirmée et résolue ; aucune décision physique n'est déduite du simple dessin.

Le bouton **Retirer la base de décision** permet de rétracter une attestation : la conformité redevient indéterminée, sans modifier la mesure ni ses générations géométriques. Préparer une nouvelle revue ne retire pas l'ancienne tant qu'aucune action explicite n'est appliquée.

Le contour, l'étalonnage, le groupe et la mesure native ne sont pas modifiés par la revue documentaire. Le MCP lit la même décision réévaluée via `get_inspection_state` ; aucune commande MCP ne peut accorder la revue humaine. Les détails donnent limites, U fournie, intervalle et motifs. Une nouvelle revue du document peut suffire après un changement documentaire ; des pixels ou une géométrie modifiés demandent une nouvelle observation. La préparation conserve son contexte et refuse une capture, révision, source PDF, image ou entrée modifiée au lieu de se réactualiser silencieusement.

Le projet conserve les justificatifs, mais leur autorité de revue est réinitialisée au rechargement et les observations rechargées restent historiques. Même un fichier JSON affirmant `reviewed: true` ne peut rétablir l'autorité. Réobserver les pixels, puis revoir explicitement la base de la nouvelle observation. Les annexes PDF/PPTX et métadonnées CSV/XLSX conservent les références documentaires et U comme historique non réévalué ; elles ne promeuvent pas une ancienne décision sauvegardée en conformité courante. Les notes d'audit applicatives indiquent séparément l'état réévalué lors de l'export.

Les essais synthétiques associés à ce développement ne qualifient aucune pièce ou caméra réelle. La qualification physique du montage et le budget d'incertitude restent à réaliser et documenter dans DPM-26/DPM-15.


## Recette logicielle composée (DPM-28)

La recette comprend également onze cas PDF/MCP d'unités documentaires (DPM-36), dont un document à deux pages : déclaration anglaise/française, pouces, répétition cohérente, conflits et absence, unité inline sous conflit, tolérance mixte et sauvegarde/rechargement d'une proposition non confirmée. Les preuves se trouvent dans `unit-checks/units_summary.json` et les échanges JSON-RPC associés. Ces tests conservent la calibration indépendante et ne créent aucune mesure de pièce.

La recette inclut aussi dix-neuf PDF synthétiques DPM-34/35 dans `attachment-checks/attachment_summary.json` et leurs échanges JSON-RPC : six cas d'attache sur un tracé, onze cas de supports découpés (chaîne simple/inversée, petits/grands écarts, branche, T, croisement, T/croisement très proches d'un joint, cycle, support intermédiaire hors vue), puis deux cas avec un composant ambigu indépendant pour vérifier l'isolation des refus. Les sources PDF sont créées dans le dossier neuf de résultats ; leurs empreintes sont contrôlées après lecture. Cette vérification conserve toutes les alternatives, les écarts originaux et ne donne aucune revue automatique.

Depuis le dossier DPER_MEASURE de la version choisie, dans PowerShell, lancer :

```powershell
.\scripts\test_inspection_workflow.ps1 -OutputDirectory "$PWD\output\automation-validation\recette-01"
```

Choisir un nouveau nom de dossier à chaque exécution. La recette exige Cargo/Rust, Python avec Pillow et les outils PDF employés par l'application. Elle lance les tests, la compilation et un hôte MCP exclusivement de test sur le port **18766** ; le paramètre `-Port` accepte un autre port dédié à partir de 18765. Aucun service existant n'est arrêté : un port occupé provoque un refus. Le serveur utilisateur 8765 n'est pas utilisé. `-TargetDirectory` permet de réutiliser explicitement un cache Cargo existant.

Les résultats restent sous `OutputDirectory` : journaux de tests/compilation/hôte, `isolation_check.json`, `mcp/smoke_summary.json`, échanges JSON-RPC et fixtures synthétiques. Le sous-dossier `native-acceptance` contient `acceptance_summary.json`, le projet et ses sources synthétiques, les exports avant/après rechargement (PDF/PPTX et manifestes), CSV et XLSX. Le script vérifie leur présence et leur cohérence avant de déclarer la recette réussie. Il restaure ses variables d'environnement, arrête uniquement le processus propriétaire lancé et vérifie la conservation du profil utilisateur.

Le scénario natif traverse les vrais boutons/champs egui : vue, choix parmi plusieurs associations, revues séparées, recalage, groupe, aperçu, enregistrement, attestation documentaire, sauvegarde, ouverture dans une nouvelle application et export. Une deuxième application exécute indépendamment les opérations d'inspection MCP sur les mêmes pixels et prérequis déjà revus ; géométrie, échantillons, valeur, états et motifs sont comparés. La matrice couvre conforme, hors tolérance, chevauchement, absence de tolérance et retrait de l'attestation. Le serveur réel teste séparément le transport, l'analyse PDF effective et les refus, notamment des affirmations de qualification/U/revue inventées sur un contour valide : elles ne peuvent pas remplacer les décisions calculées par l'application.

Les rapports distinguent explicitement l'état géométrique sauvegardé, la décision sauvegardée et la décision courante réévaluée dans les notes d'audit. Un historique « conforme » ne donne aucune conformité courante au rechargement. Une base documentaire manquante n'est pas affichée comme « indéterminée » lorsque la tolérance est absente : le verdict reste « non évalué ».

Les comparaisons de points/valeurs **recalculés pour un historique importé** tiennent compte du seul arrondi machine JSON, borné à `2 × EPSILON × max(|a|, |b|)` et uniquement pour des nombres finis. Cette marge n'est ni une tolérance de pièce ni une incertitude ; elle évite de déclarer une modification inexistante après une variation d'une unité de représentation flottante. Les observations vivantes, identités, générations, empreintes de pixels/calibration et blocages d'import/revue restent stricts. Toute preuve importée reste à réobserver.

Cette recette valide le parcours logiciel assisté actuellement supporté. Le scénario natif utilise une interprétation textuelle et des propositions SVG fournies ; il ne démontre pas la reconnaissance sémantique d'un plan quelconque. Les revues sont des événements d'interface dans un test, pas une qualification réelle. Le scénario natif isole certaines portes de qualité d'image ; le transport MCP vérifie séparément les contrôles de qualité. Aucune fenêtre de bureau, caméra ou chaîne optique réelle n'est qualifiée. DPM-26 reste nécessaire pour les essais physiques ; les développements de reconnaissance et d'appariement automatiques restent dans leurs tickets dédiés.
