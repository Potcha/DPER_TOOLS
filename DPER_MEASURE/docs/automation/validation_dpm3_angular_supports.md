# DPM-3 — diagnostic des supports angulaires du PDF

## Fonction livrée et limites

`inspect_drawing_angular_supports` examine une cote angulaire explicite en degrés et deux primitives `Line` choisies dans la même vue PDF courante. La réponse conserve la cote source, les deux primitives complètes, leurs empreintes, la preuve de vue, la justification et les limites. L'intersection est exprimée en points PDF ; les deux angles supplémentaires possibles sont exprimés en degrés.

Les supports doivent être ouverts, à exactement deux points finis et de longueur supérieure à 1e-6 point PDF. L'angle aigu doit être au moins 15°. L'intersection doit être sur la page et appartenir aux deux segments, avec une tolérance d'extrémité de 1e-6 point PDF. Ce seuil documentaire n'est pas une précision physique. Les supports presque parallèles et ceux nécessitant une extrapolation sont hors domaine. Les courbes, polylignes et formes fermées sont refusées.

Le diagnostic ne reconnaît pas les rôles des lignes de cote et des contours de pièce. Il ne choisit aucun secteur angulaire, même pour un angle droit ; `selected_angle_deg` reste nul. Le nominal n'intervient jamais dans le calcul ou le choix. Le résultat est `documentary_proposal`, avec `reviewed=false` et `measured_on_photo=false`.

Une déclaration documentaire cohérente en millimètres n'annule pas une cote explicitement en degrés. Les contextes documentaires contradictoires sont refusés. Capture, calibration, mesures, associations persistantes et conformité ne sont pas modifiées par le diagnostic. DPM-3 reste ouvert pour les associations persistantes des cotes linéaires/angulaires et la reconnaissance des vues/rôles.

## Protocole de validation manuelle dès la livraison

1. Fermer l'ancienne instance et lancer :

   ```powershell
   & 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm3-angular-app\debug\dper_measure.exe'
   ```

   L'installateur existant ne contient pas cette évolution. Aucune image ni calibration n'est nécessaire pour ce diagnostic documentaire.
2. Importer comme **plan de référence**, et non comme image de la pièce :

   `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm3-angular-manual\supports-angulaires.pdf`

   Le fichier est déjà généré localement. Pour le régénérer dans un **nouveau dossier** :

   ```powershell
   Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
   python scripts/verify_angular_drawing_supports_mcp.py output/dpm3-angular-manual-bis
   ```

3. Demander au chat : **« Exécute la recette DPM-3 des supports angulaires sur le plan ouvert »**.
4. Le chat vérifie le plan actif, analyse les géométries de la page 1, récupère les primitives complètes et prépare explicitement une vue contenant les caractéristiques angulaire et linéaire, et les supports définis ci-dessous. Cette préparation modifie le plan/la vue et la révision. Il relève ensuite `get_inspection_state` et `get_measurement_context` comme références. Chaque appel de diagnostic doit conserver ces deux états.
5. Attendre les résultats du tableau. Aucun tracé sur photo ni nouvel écran n'est attendu. Les preuves se consultent dans les réponses MCP. Ne pas interpréter les 45°/135° comme une mesure physique ou une conformité à 123°.

| Cas | Résultat attendu |
| --- | --- |
| Supports A et B | Intersection (120,100) pt, angles [45,135] degrés, secteur non sélectionné |
| B puis A | Même intersection et mêmes angles |
| A puis B retourné | Même intersection et mêmes angles |
| A et parallèle | Refus |
| A et support éloigné | Refus d'extrapolation |
| A et presque parallèle | Refus (angle aigu inférieur à 15°) |
| Support exclu de la vue | Refus |
| Deux fois A | Refus |
| Cote linéaire 18 mm | Refus de famille de cote |
| Révision précédente | Refus |
| Justification vide | Refus |
| Champ supplémentaire reviewed=true | Refus du schéma |

La comparaison numérique admet 0,01 pt et 0,01° pour cette fixture. La cote reste nominalement **123 ±0,1°**, sans choix forcé de l'un des angles proposés.

### Paramètres MCP et identification explicite

Appeler `analyze_drawing_geometry` avec `page:1` et `expected_revision` courant. Les réponses de liste de `get_drawing_geometries` sont des résumés : rappeler l'outil avec chaque `geometry_id` pour obtenir les points complets, sans fabriquer les identifiants.

| Support | Premier point (pt) | Second point (pt) | Inclus dans la vue |
| --- | --- | --- | --- |
| A | (60,100) | (180,100) | Oui |
| B | (80,60) | (160,140) | Oui |
| Parallèle | (60,160) | (180,160) | Oui |
| Éloigné | (240,140) | (260,160) | Oui |
| Presque parallèle | (60,99) | (180,101) | Oui |
| Exclu | (300,80) | (300,150) | Non |
| B retourné | (160,140) | (80,60) | Oui |

Définir la vue avec `define_drawing_view`, ses deux `feature_ids` et les six `geometry_ids` inclus. Reprendre la révision renvoyée. Appeler ensuite `inspect_drawing_angular_supports` avec les champs :

```text
expected_revision : révision courante après préparation
feature_id : identifiant réel de la cote angulaire 123°
view_id : identifiant de la vue préparée
support_ids : les deux identifiants réels des supports A et B
justification : « Supports identifiés explicitement dans le PDF synthétique ; secteur non choisi. »
```

Le treizième cas automatique remplace les octets du PDF synthétique de test et vérifie le refus de réutilisation de sa vue. Il est exécuté sur une **copie locale dédiée dans l'hôte de test**. Pour la recette sur le plan ouvert, conserver le fichier de l'utilisateur intact ; les douze premiers cas suffisent à la vérification immédiate. Aucun fichier industriel n'est requis.

## Vérifications réellement exécutées le 10 novembre 2026

- `cargo test --locked drawing_angular_supports::` : **9 nouveaux tests réussis**, zéro échec, 412 filtrés. Calcul indépendant du nominal, angles droit et supplémentaires, permutation/retournement/rotation, limites, refus, sources et unités couverts.
- `cargo test --locked drawing_linear_supports::` : **9 tests liés réussis**, zéro échec.
- `cargo test --locked dper_binding::tests::` : **9 tests liés réussis**, zéro échec. Soit **27 tests Rust distincts**, sans suite complète.
- Recette réelle via MCP : **13 cas réussis**, zéro échec, dont trois diagnostics et dix refus. PDF réellement extrait par Poppler. État mesure et inspection inchangé après chaque diagnostic, hormis le statut de vue recalculé après modification externe du PDF. La révision stockée reste inchangée dans ce cas.
- Hôte propriétaire 18771 exécuté explicitement puis arrêté avec succès. Serveur utilisateur 8765 non utilisé. Preuves : `output/dpm3-angular-mcp-20261110-104332/recipe/summary.json` et journal RPC dans le même dossier.
- Compilation debug réussie dans `output/dpm3-angular-app`, 40 avertissements existants de code inutilisé, 30 côté tests. Journaux `output/dpm3-angular-tests.log`, `output/dpm3-angular-linear-related.log`, `output/dpm3-angular-binding-related.log`, `output/dpm3-angular-build.log`.
- PDF manuel rendu et inspecté : textes et traits lisibles. Avertissement Poppler sur la police Symbol, sans glyphe manquant visible.
- Relecture indépendante statique sans défaut concret relevé. Aucun essai physique, caméra/GPU ou qualification métrologique ; données synthétiques uniquement.
