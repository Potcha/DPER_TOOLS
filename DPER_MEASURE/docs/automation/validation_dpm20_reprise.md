# DPM-20 — Reprise guidée du contrôle

## Portée

L'interface et `get_inspection_state.characteristic_progress` indiquent désormais la prochaine action de chaque caractéristique. Le résultat est recalculé depuis les preuves courantes : interprétation, association, occurrence manquante, ambiguïté, tolérance ou décision à examiner. Une indication n'exécute aucune validation physique.

Pour un rayon ou diamètre, « Préparer la prochaine observation » prépare le brouillon avec l'association existante et la première occurrence manquante. Le groupe existant de cette caractéristique et de cette capture est repris. Un brouillon déjà ouvert est conservé et signalé. L'aperçu et l'enregistrement restent deux actions distinctes. Une affectation périmée demande une nouvelle affectation plutôt qu'une observation sur une ancienne source.

## Rejouer les vérifications logicielles

Dans PowerShell :

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
cargo test --locked inspection_progress::
cargo test --locked dpm20_guided_preparation
cargo test --locked dpm20_guidance_targets
cargo test --locked composed_native_
cargo test --locked dpm20_report_coverage
```

Ces tests utilisent leurs propres données synthétiques. Aucun fichier industriel ni serveur utilisateur n'est nécessaire. Le parcours composé manipule egui et l'adaptateur applicatif MCP, sauvegarde, rouvre et exporte ; ce n'est pas un nouvel essai du transport HTTP réel ni de la fenêtre Windows.

## Recette dans l'application

1. Enregistrer le projet utilisateur puis fermer son ancienne instance. Lancer `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`, construit depuis `codex/DPM-20-resumable-guidance` ou depuis `dev` après fusion. L'installateur antérieur ne contient pas cette livraison.
2. Utiliser un projet d'inspection possédant une photo calibrée, une association circulaire et un recalage déjà préparés. Le plan synthétique disponible pour examiner le panneau seul est `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\tests\fixtures\drawing_geometry\01_circle_leader.pdf`. Ce PDF seul ne fournit ni photo ni étalonnage et ne permet pas le parcours d'observation complet.
3. Ouvrir « Plan de contrôle et inspection IA », puis « Avancement du contrôle ». Vérifier une prochaine étape par cote. Une interprétation non confirmée demande sa revue ; une cote non vérifiable en 2D demande une autre méthode. Une absence de tolérance ne doit pas devenir une tolérance nulle.
4. Pour une cote circulaire confirmée et associée, sans observation courante, cliquer « Préparer la prochaine observation ». Dans « Observer les contours de la photo », vérifier la caractéristique, l'occurrence éventuelle et le groupe. Aucun nouveau tracé ne doit être enregistré à cette étape.
5. Prévisualiser les contours, examiner les pixels et les preuves, puis enregistrer explicitement. Vérifier que la prochaine étape se met à jour. Pour une cote répétée, elle doit viser le premier emplacement restant, sans recompter un aperçu ou un doublon comme une nouvelle occurrence.
6. Avec un brouillon ouvert, redemander une préparation guidée : le brouillon doit être conservé et un message doit signaler qu'il est déjà ouvert.
7. Sur une copie de projet et de PDF, remplacer le PDF source puis réexaminer l'avancement. L'affectation doit être suspendue ; réanalyser et renouveler le choix avant de reprendre. Aucune confirmation ne doit être accordée automatiquement.
8. Facultatif : demander `get_inspection_state` au client MCP et comparer `characteristic_progress[].next_action` avec les instructions visibles. Cet appel ne doit pas ajouter de mesure.

## Résultats réellement obtenus

- 5 tests `inspection_progress::`, 1 test de préparation, 1 test d'occurrence manquante/source périmée, 2 tests de parcours composé et 1 test de couverture du rapport : **10 tests distincts réussis**.
- Deux premiers essais ont échoué : préparation de fixture sans choix d'occurrence (test corrigé), et erreur Windows 650 sur les fichiers temporaires d'export (rejeu hors restriction réussi).
- Compilation debug réussie : 38 avertissements de code inutilisé ; compilation des tests : 30 avertissements.
- Journaux locaux dans `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\` : `dpm20-guidance-tests.log`, `dpm20-guided-preparation.log`, `dpm20-missing-member.log`, `dpm20-composed.log`, `dpm20-report-linked.log`, `dpm20-guidance-build.log`.
- Relecture indépendante ciblée ; défaut d'affectation périmée corrigé puis couvert par régression.

Aucune suite complète relancée, aucune manipulation de la fenêtre Windows ni qualification physique revendiquée. DPM-20 reste ouvert pour l'orchestration complète ; cette tranche guide et prépare les observations circulaires existantes, elle ne détecte pas automatiquement toutes les géométries d'un plan quelconque.
