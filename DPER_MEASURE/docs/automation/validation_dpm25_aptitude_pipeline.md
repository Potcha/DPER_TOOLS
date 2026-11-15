# DPM-25 — Preuves d’aptitude et limites du pipeline

## Périmètre

Recette exécutée le 15 novembre 2026 sur `dev` après la PR #75
(`54bbf76`), avec les correctifs joints à cette livraison.
Le dossier regroupe les preuves de calcul synthétique, de
fonctionnement applicatif et de l’essai physique limité accepté dans DPM-26.

| Domaine | Référence et preuve | Portée |
| --- | --- | --- |
| Calcul synthétique | Géométries générées avec des paramètres connus avant ajustement. Les tests couvrent notamment droites, cercles, arcs, trapèze guidé, homographie et similitude. | Les seuils et écarts s’appliquent aux jeux d’essai et aux configurations vérifiés. |
| Application | Recette DPM-28 rejouée avec manipulations egui, rejeu indépendant via l’adaptateur MCP, sauvegarde, réouverture et exports. Le transport HTTP exerce séparément l’extraction PDF, les observations et les refus. | Le parcours natif se sert de prérequis documentaires explicites. Les décisions de la recette utilisent une qualification et une incertitude fournies à titre fictif. |
| Essai physique limité | DPM-26 a été accepté et clôturé sur décision de l’utilisateur pour une ouverture déclarée de 0,2 mm. Le recalcul du segment d’étalonnage retrouve 0,2 mm. Le diagnostic Python local donne une médiane de 0,2052 mm sur 28 profils de la même image. | La même ouverture sert à établir l’échelle. Les profils appartiennent à une seule acquisition. La justesse sur référence indépendante, la répétabilité entre acquisitions, les variations dans le champ et l’incertitude totale restent à établir. [Acceptation DPM-26](https://dipper.atlassian.net/browse/DPM-26?focusedCommentId=13646) |

La recette vérifie un contrôle assisté sur des cas documentés. La lecture
générale de plans, l’orchestration complète DPM-20, les assemblages et
l’emploi du diagnostic projectif sur une chaîne physique demandent leurs
propres preuves. Le statut physique du projet de DPM-26 reste
`single_reference_unverified`. La décision de conformité d’une pièce
réelle exige des preuves applicables à son mesurande et à ses conditions
de mesure.

## Écarts découverts

Au premier passage, 510 tests réussissent, un échoue et l’hôte dédié reste
ignoré à ce stade. Pendant la sélection d’un repère photo ou l’attente du
relâchement du bouton, les actions MCP étaient correctement bloquées.
Le motif générique ne désignait plus l’action à terminer. Le correctif
rétablit le motif spécifique et conserve le motif générique pendant le
redimensionnement numérique. Le test existant couvre les deux phases
du geste et l’intégrité des données pendant les refus.

Le passage suivant réussit la suite Rust et la compilation. La recette
HTTP s’arrête sur une assertion exigeant exactement le schéma 20,
alors que l’application sauvegarde au schéma 22. Les deux recettes
concernées acceptent le schéma de persistance à partir de 20 et
vérifient toujours le contenu des preuves après réouverture.

## Résultats du passage final

- 511 tests Rust réussis, zéro échec. L’hôte ignoré par défaut a ensuite
  été exécuté avec succès. La suite locale inclut une extension
  préexistante du test de couverture des rapports DPM-20, hors de cette PR.
- Compilation debug réussie avec 38 avertissements de code inutilisé.
- Interface egui et rejeu indépendant via l’adaptateur MCP en accord.
  Le disque analytique de 4,5 mm donne 4,501407 mm. L’état de décision
  redevient indéterminé après réouverture. PDF, PPTX, CSV et XLSX exercés.
- Sur le transport HTTP MCP, 18 outils détectés, sept cas adverses de
  vision, huit dégagements répartis en deux groupes, 19 cas d’attache PDF,
  11 cas d’unités et 17 cas de pointes. Le disque synthétique de 4,2 mm
  donne 4,207240 mm pour un nominal de plan de 4,0 mm. Les deux
  occurrences distinctes donnent 3,507436 mm et 3,227360 mm pour des
  références synthétiques de 3,50 mm et 3,22 mm.
- Arrêt normal de l’hôte dédié et profil utilisateur inchangé.
  [Synthèse et empreintes des preuves](evidence/dpm25.json)

## Rejeu sous Windows

- Dossier de travail `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`
- Lanceur `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\scripts\test_inspection_workflow.ps1`
- Exécutable debug `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`
- Exécution finale `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm25-workflow-20261115-163253-547`
- Essai physique déjà accepté, conservé localement dans `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm26-acceptation-20261114`

Depuis la branche de la livraison ou depuis `dev` après fusion, ouvrir
PowerShell avec Rust, Python, Pillow et Poppler disponibles. Le port
18780 doit être libre. Le lanceur crée ses entrées synthétiques dans un
dossier neuf, compile l’application et lance son hôte de test isolé.

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
$destination = Join-Path (Get-Location) ('output\dpm25-rejeu-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
& .\scripts\test_inspection_workflow.ps1 -OutputDirectory $destination -Port 18780
```

Consulter `tests.log`, `isolation_check.json`, les synthèses de
`native-acceptance` et `mcp\smoke_summary.json`. Les sous-dossiers
`clearance-checks`, `attachment-checks`, `unit-checks` et `tip-checks`
contiennent les cas complémentaires. Les journaux bruts et les exports
restent dans le dossier local. La recette utilise des contrôles egui et
un hôte MCP sans fenêtre. Le lancement interactif Windows et
l’installation du candidat restent des vérifications de livraison.
