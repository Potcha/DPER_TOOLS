# Utiliser l’incrément 1 d’inspection assistée

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

## Limites actuelles à conserver dans le compte rendu

- Le PDF est interprété conservativement à partir du texte disponible ; l’association sémantique complète entre vues, lignes d’attache et caractéristiques n’est pas accomplie.
- Les correspondances plan/photo et les propositions locales restent explicites ; aucun appariement automatique universel n’est fourni.
- Le service d’inspection ne dispose pas encore d’une qualification physique ni de l’incertitude totale par mesurande : il force ces informations à « non qualifiée » et « inconnue ». Il ne doit donc pas annoncer une conformité physique, même avec un contour logiciellement validé.
- Une U fournie pour la calibration ne remplace pas le budget de la mesure. Une incertitude en millimètres ne devient pas une incertitude en degrés.
- Un cercle de jeu tangent à **quatre butées** demande un modèle et des observations dédiés ; l’ajustement d’un contour circulaire ne l’implémente pas.
- Les profondeurs, faces cachées, dimensions hors plan et perspectives non justifiées restent impossibles à valider depuis cette seule méthode 2D.

Les essais effectivement réalisés dans l’application relancée, les tests logiciels et les essais physiques doivent faire l’objet de comptes rendus séparés. Ce guide n’annonce aucun de ces contrôles comme exécuté.
