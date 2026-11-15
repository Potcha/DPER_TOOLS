# DPM-20 — Recette du plan au rapport

Recette exécutée le 16 novembre 2026 sur `dev` après la fusion de la PR #87
(`1ca473c`). Une fenêtre Windows dédiée a servi aux confirmations humaines.
Le client HTTP MCP a préparé le plan, exercé les opérations métier et relevé
l’état du même projet à chaque étape. Les sources et la base de décision
sont synthétiques.

| Étape | Résultat observé |
| --- | --- |
| Importer le projet étalonné et analyser le PDF | Les deux exigences restent visibles. Le diamètre de 3,2 ± 0,4 mm attend sa confirmation. La profondeur de 8 mm est signalée hors de portée de l’image 2D. |
| Confirmer le diamètre dans Windows, analyser la page et définir sa vue | La recherche présente quatre hypothèses de recalage. Une demande sans candidat explicite est refusée et l’état reste identique. |
| Choisir et justifier la première hypothèse | Le candidat `candidate-002` et les quatre alternatives sont conservés. L’aperçu ne crée aucune mesure. L’enregistrement explicite crée M1. La construction M2 dépend de M1. |
| Relancer la recherche et corriger le choix | Le candidat `candidate-011` est enregistré avec son motif. M1 et M2 deviennent périmées. Le contrôle compte zéro observation courante avant la reprise. |
| Reprendre l’observation | Un nouvel aperçu puis un enregistrement explicite créent M3. La photo donne 3,001003 mm pour un nominal de 3,2 mm. Les preuves de M1 et les deux choix de recalage restent disponibles. |
| Revoir M3 dans Windows | La décision simulée est conforme après revue avec une incertitude totale déclarée de 0,05 mm. La profondeur conserve son motif d’abstention. |
| Sauvegarder et exporter | PDF de six pages et manifeste vérifiés. Les choix, motifs, points acceptés, valeur observée, décision sauvegardée, construction périmée et profondeur hors 2D restent traçables. Le PDF identifie les décisions sauvegardées comme historiques. |
| Rouvrir et reprendre | Les deux choix et les observations sont relus comme historique. Réanalyser la page puis revoir le recalage permet d’enregistrer une nouvelle observation M4. |

Le test existant
`composed_native_workflow_decision_independent_mcp_replay_save_reopen_and_exports`
a également réussi sur cette version. Il compare le parcours piloté par les
contrôles natifs à un rejeu indépendant via l’adaptateur MCP, puis vérifie
la sauvegarde, la réouverture et les exports.

## Rejeu et preuves

Le dossier de recette est
`O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-composed`.
Il contient `plan.pdf`, `photo.png`, `fixture.dperproj`, `recipe.py`, les
journaux `rpc_evidence_*.json` et `native-parity.log`.
L’exécutable est
`O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`.

1. Démarrer une instance dédiée avec `DPER_MEASURE_MCP_PORT=18790` et un profil
   temporaire. Depuis la racine du dépôt de travail, exécuter
   `python output/dpm20-composed/recipe.py setup`.
2. Dans **Plan de référence**, ouvrir la cote du diamètre et cocher
   **Interprétation du plan vérifiée**.
3. Exécuter les étapes `first` puis `correct` de `recipe.py`. Vérifier
   **Valeur suspendue** pour M1 et **À recalculer** pour M2. La capture
   `apres-correction.png` montre ces deux états et la nouvelle mesure.
4. Dans **Base documentaire de décision de conformité**, sélectionner M3,
   préparer la revue, renseigner la référence synthétique `SYNTH-DPM20`,
   la portée, l’incertitude totale de 0,05 mm, sa source et le responsable,
   puis appliquer la base. Le résultat apparaît dans `decision.png`.
5. Exécuter `report` puis `reopen`. Consulter `convergence.dperproj`,
   `convergence.pdf`, `convergence.pdf.dper_measure.json`, `results.json`
   et `reopen.json`. Le projet repris a été sauvegardé dans `reprise.dperproj`.
   La page 3 du rapport a été rendue et inspectée. Poppler a signalé un
   avertissement concernant la police Symbol. L’instance dédiée a été fermée.

La recette couvre le parcours circulaire assisté et l’abstention sur une
profondeur. Les extensions de formes relèvent de DPM-12 et la qualification
physique relève du périmètre métrologique dédié.
