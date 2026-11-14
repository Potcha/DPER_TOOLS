# DPM-20 — Aperçus groupés des contours circulaires

## Livraison et limites

Branche `codex/DPM-20-control-plan`, fondée sur `dev` après fusion de la PR #69 (`7b1ecdf`). Le bouton « Prévisualiser les contours prêts (8 maximum) » et l'appel MCP `preview_control_plan` exécutent la même opération applicative.

L'opération examine toutes les caractéristiques du plan courant et tente au plus huit recherches de diamètres/rayons manquants et déjà associés. Pour une répétition, les membres manquants sont recensés avant toute recherche : un aperçu n'est pas une occurrence mesurée. Les autres caractéristiques restent présentes dans l'avancement avec leur prochaine action ; elles ne sont pas considérées contrôlées. Une erreur de contour ne supprime pas les autres recherches.

Les aperçus ajoutent des preuves et font évoluer la révision de session. Ils n'ajoutent aucune mesure native, ne confirment aucune interprétation et ne délivrent aucune conformité. Le nominal n'est jamais utilisé comme résultat mesuré. Un brouillon d'observation ouvert empêche le traitement groupé et reste intact.

La limite n'est pas une pagination : un nouvel appel peut refaire les premiers aperçus tant que leurs observations ne sont pas enregistrées. Pour traiter les recherches `pending_limit`, utiliser la préparation individuelle, ou enregistrer explicitement les premières observations après revue avant de relancer. L'enregistrement individuel exige toujours son propre aperçu courant. Les autres familles de géométries, l'enregistrement automatique et l'orchestration complète restent hors de cet incrément.

## Vérifications reproductibles

Depuis PowerShell :

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
cargo test --locked batch_preview_control_plan
cargo test --locked inspection_progress::tests
cargo build --locked
```

Les quatre nouveaux tests couvrent les occurrences distinctes sans mesure enregistrée, la révision périmée, la conservation d'un brouillon, l'échec local suivi d'un succès, une caractéristique bloquée, les limites 0/9 refusées et la limite 1 avec un membre en attente. Le dernier passe par l'adaptateur de requête partagé avec MCP. Les cinq tests d'avancement existants sont liés à ce traitement.

## Recette dans l'application

1. Enregistrer le projet utilisateur avant de fermer son ancienne instance. Lancer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe` après compilation de cette branche, ou de `dev` après fusion. L'ancien installateur ne contient pas cette modification.
2. Ouvrir une copie d'un projet avec photo figée et calibrée, PDF analysé, interprétations et associations circulaires déjà revues, recalage courant et, pour les répétitions, affectation explicite des occurrences. Aucun fichier industriel n'est distribué avec cette livraison. Le PDF `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\tests\fixtures\drawing_geometry\01_circle_leader.pdf` permet d'examiner le panneau documentaire, mais ne fournit pas à lui seul cette préparation ni une photo calibrée.
3. Noter le nombre de mesures. Dans « Plan de contrôle et inspection IA », cliquer « Prévisualiser les contours prêts (8 maximum) ». Vérifier les compteurs de réussites, erreurs et attentes, puis les preuves et l'avancement. Le nombre de mesures doit rester identique.
4. Examiner les contours sur les pixels et leurs limites. Une cote sans interprétation confirmée, association ou sources courantes ne doit pas devenir une observation validée. Une cote non prise en charge doit rester visible et demander l'action appropriée.
5. Préparer individuellement une observation : caractéristique, occurrence et groupe doivent correspondre au choix courant. Prévisualiser de nouveau, examiner puis enregistrer explicitement. Une seule nouvelle mesure doit être créée ; les autres aperçus ne doivent pas être enregistrés implicitement.
6. Avec ce brouillon encore ouvert, cliquer sur le bouton groupé : refus explicite, brouillon conservé et aucune mesure ajoutée.
7. Sur une copie de projet, essayer un contour absent parmi plusieurs emplacements : le motif de l'erreur doit être visible et les recherches indépendantes doivent continuer. Pour plus de huit recherches, les autres restent en attente et ne sont pas présentées comme réalisées.

## Recette MCP

Dans le client relié à cette version, appeler `get_measurement_context`, puis copier `capture_id` et `document_revision` dans les champs ci-dessous (remplacer les valeurs d'exemple) :

```json
{
  "capture_id": "ID_DE_LA_CAPTURE",
  "expected_revision": 123,
  "limit": 8
}
```

Appeler `preview_control_plan` avec ces arguments. Vérifier `jobs` (`previewed`, `error`, `pending_limit`), `blocked`, `characteristic_progress` et la nouvelle `document_revision`. Une ancienne révision doit être refusée. `get_measurement_context` doit confirmer que les mesures n'ont pas augmenté. L'appel n'est pas en lecture seule : les preuves et la révision peuvent changer.

## Preuves et statut

Quatre tests groupés et cinq tests d'avancement réussis lors de cet incrément. La première exécution a révélé un identifiant dupliqué dans la fixture de test ; corrigé avant réussite finale. Compilation debug réussie en 26,49 secondes, avec 39 avertissements de code inutilisé. Le journal de compilation se trouve dans `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-control-plan-build.log`.

Pas de suite complète relancée ni de nouvel essai HTTP MCP, de fenêtre Windows ou de qualification physique revendiqué. DPM-20 reste ouvert pour l'orchestration complète et sa recette finale ; ce lot réutilise les moteurs existants pour regrouper les aperçus circulaires.
